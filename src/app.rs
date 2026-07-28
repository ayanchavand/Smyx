//! Application state and navigation business logic.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use ratatui_image::picker::Picker;

use crate::anim::ThemeFade;
use crate::engine::Engine;
use crate::login_modal::LoginModalState;
use crate::models::{
    ActionMenu, Activated, Detail, Library, LibItem, NowPlaying, PlaySource,
    RightView, Section, SortMode, FADE_MS,
};
use crate::subsonic::SubsonicClient;
use crate::theme::Theme;

pub struct App {
    pub engine: Engine,
    pub picker: Picker,
    pub displayed: Theme,
    pub target: Theme,
    pub fade: Option<ThemeFade>,
    pub now: Option<NowPlaying>,
    pub subsonic: Arc<Mutex<Option<SubsonicClient>>>,
    pub login_modal: Option<LoginModalState>,
    pub status: String,
    pub library: Library,
    pub section: Section,
    pub selected: usize,
    pub shuffle: bool,
    pub repeat: bool,
    pub volume: u8,
    pub queue: Vec<String>,
    pub queue_uris: Vec<String>,
    pub input_mode: bool,
    pub query: String,
    pub searching: bool,
    pub search_results: Vec<LibItem>,
    pub lyrics: Vec<(u32, String)>,
    pub lyrics_synced: bool,
    pub view: RightView,
    pub details: Vec<Detail>,
    pub actions: Option<ActionMenu>,
    pub pending_meta: Option<String>,
    pub art_dirty: u8,
    pub playback_started: bool,
    pub source: PlaySource,
    pub source_name: String,
    pub sort: SortMode,
    pub bar_rect: Option<Rect>,
    pub scroll_rect: Option<Rect>,
    pub scroll_len: usize,
    pub vol_rect: Option<Rect>,
    pub last_ctrl_c: Option<Instant>,
    pub tab_rects: Vec<(RightView, Rect)>,
    pub lib_rect: Option<Rect>,
    pub lib_offset: usize,
    pub last_click: Option<(u16, Instant)>,
}

impl App {
    pub fn new(
        engine: Engine,
        picker: Picker,
        initial_theme: Theme,
        subsonic: Arc<Mutex<Option<SubsonicClient>>>,
        login_modal: Option<LoginModalState>,
        volume: u8,
    ) -> Self {
        Self {
            engine,
            picker,
            displayed: initial_theme,
            target: initial_theme,
            fade: None,
            now: None,
            subsonic,
            login_modal,
            status: String::new(),
            library: Library::default(),
            section: Section::Home,
            selected: 0,
            shuffle: false,
            repeat: false,
            volume,
            queue: Vec::new(),
            queue_uris: Vec::new(),
            input_mode: false,
            query: String::new(),
            searching: false,
            search_results: Vec::new(),
            lyrics: Vec::new(),
            lyrics_synced: false,
            view: RightView::NowPlaying,
            details: Vec::new(),
            actions: None,
            pending_meta: None,
            art_dirty: 0,
            playback_started: false,
            source: PlaySource::None,
            source_name: String::new(),
            sort: SortMode::Added,
            bar_rect: None,
            scroll_rect: None,
            scroll_len: 0,
            vol_rect: None,
            last_ctrl_c: None,
            tab_rects: Vec::new(),
            lib_rect: None,
            lib_offset: 0,
            last_click: None,
        }
    }

    pub fn start_fade(&mut self, to: Theme) {
        self.fade = Some(ThemeFade::new(
            self.displayed,
            to,
            Duration::from_millis(FADE_MS),
        ));
        self.target = to;
    }

    pub fn cur_items(&self) -> &[LibItem] {
        if let Some(d) = self.details.last() {
            &d.items
        } else if self.searching {
            &self.search_results
        } else {
            self.library.items(self.section)
        }
    }

    pub fn cur_list_mut(&mut self) -> &mut Vec<LibItem> {
        if let Some(d) = self.details.last_mut() {
            &mut d.items
        } else if self.searching {
            &mut self.search_results
        } else {
            self.library.items_mut(self.section)
        }
    }

    pub fn position_ms(&self) -> u32 {
        match &self.now {
            Some(n) if n.is_playing => {
                (n.position_ms + n.position_at.elapsed().as_millis() as u32).min(n.duration_ms)
            }
            Some(n) => n.position_ms.min(n.duration_ms),
            None => 0,
        }
    }

    pub fn seek_to(&mut self, position_ms: u32) {
        let Some(dur) = self.now.as_ref().map(|n| n.duration_ms) else {
            return;
        };
        let new = position_ms.min(dur);
        let _ = self.engine.seek(new);
        if let Some(n) = self.now.as_mut() {
            n.position_ms = new;
            n.position_at = Instant::now();
        }
    }

    pub fn seek_by(&mut self, delta_ms: i64) {
        let cur = self.position_ms() as i64;
        self.seek_to((cur + delta_ms).max(0) as u32);
    }

    pub fn first_selectable(&self) -> usize {
        self.cur_items()
            .iter()
            .position(|i| !i.is_header)
            .unwrap_or(0)
    }

    pub fn move_sel(&mut self, dir: isize) {
        let items = self.cur_items();
        let n = items.len() as isize;
        if n == 0 {
            return;
        }
        let mut i = self.selected as isize;
        loop {
            i += dir;
            if i < 0 || i >= n {
                return;
            }
            if !items[i as usize].is_header {
                self.selected = i as usize;
                return;
            }
        }
    }

    pub fn normalize_selection(&mut self) {
        if self
            .cur_items()
            .get(self.selected)
            .is_some_and(|i| i.is_header)
        {
            self.selected = self.first_selectable();
        }
    }

    pub fn play_context_row(&mut self, uri: String, name: String, shuffle: bool) {
        self.status = format!("starting {name}…");
        self.source = PlaySource::Context(uri.clone());
        self.source_name = name;
        if let Err(e) = self.engine.play_context(uri, shuffle) {
            self.status = format!("couldn't play: {e:#}");
        }
    }

    pub fn activate(&mut self) -> Activated {
        let Some(item) = self.cur_items().get(self.selected).cloned() else {
            return Activated::None;
        };
        if item.is_header {
            return Activated::None;
        }
        if item.is_play {
            if item.uri == "myx:action:liked-play" {
                let uris: Vec<String> = self
                    .library
                    .liked
                    .iter()
                    .filter(|i| i.is_track)
                    .map(|i| i.uri.clone())
                    .collect();
                if !uris.is_empty() {
                    self.source = PlaySource::Liked;
                    self.source_name = "Liked Songs".to_string();
                    self.status = "starting Liked Songs…".to_string();
                    if let Err(e) = self.engine.play_tracks(uris, None, 0, self.shuffle) {
                        self.status = format!("couldn't play: {e:#}");
                    }
                }
                return Activated::None;
            }
            let name = self
                .details
                .last()
                .map(|d| d.title.clone())
                .unwrap_or_else(|| item.name.clone());
            let shuffle = self.shuffle;
            self.play_context_row(item.uri, name, shuffle);
            return Activated::None;
        }
        if item.is_track {
            if let Some(id) = item.uri.strip_prefix("subsonic:track:") {
                if let Err(e) = self.engine.play_track_id(id) {
                    self.status = format!("Playback error: {}", e);
                } else {
                    self.now = Some(NowPlaying {
                        uri: item.uri.clone(),
                        title: item.name.clone(),
                        artist: item.subtitle.clone(),
                        album: String::new(),
                        duration_ms: 0,
                        position_ms: 0,
                        position_at: Instant::now(),
                        is_playing: true,
                        cover: None,
                    });
                    self.playback_started = true;
                    self.status = format!("Playing {}", item.name);
                }
            }
            return Activated::None;
        }
        if let Some((uri, name)) = context_target(&item) {
            return Activated::Open(uri, name);
        }
        Activated::None
    }

    pub fn play_item(&mut self, item: &LibItem) {
        if item.is_track {
            if let Some(id) = item.uri.strip_prefix("subsonic:track:") {
                if let Err(e) = self.engine.play_track_id(id) {
                    self.status = format!("Playback error: {}", e);
                } else {
                    self.now = Some(NowPlaying {
                        uri: item.uri.clone(),
                        title: item.name.clone(),
                        artist: item.subtitle.clone(),
                        album: String::new(),
                        duration_ms: 0,
                        position_ms: 0,
                        position_at: Instant::now(),
                        is_playing: true,
                        cover: None,
                    });
                    self.playback_started = true;
                    self.status = format!("Playing {}", item.name);
                }
            }
        }
    }

    pub fn play_next(&mut self) {
        if !self.queue_uris.is_empty() {
            let next_uri = self.queue_uris.remove(0);
            if !self.queue.is_empty() {
                self.queue.remove(0);
            }
            let name = self
                .cur_items()
                .iter()
                .find(|i| i.uri == next_uri)
                .map(|i| i.name.clone())
                .unwrap_or_else(|| "Queued Track".to_string());
            let subtitle = self
                .cur_items()
                .iter()
                .find(|i| i.uri == next_uri)
                .map(|i| i.subtitle.clone())
                .unwrap_or_default();

            let item = LibItem::track(name, subtitle, next_uri);
            self.play_item(&item);
            return;
        }

        let items = self.cur_items().to_vec();
        let tracks: Vec<(usize, &LibItem)> = items
            .iter()
            .enumerate()
            .filter(|(_, i)| i.is_track && !i.is_header)
            .collect();
        if tracks.is_empty() {
            return;
        }

        let curr_uri = self.now.as_ref().map(|n| n.uri.as_str());
        let curr_idx = curr_uri.and_then(|uri| tracks.iter().position(|(_, item)| item.uri == uri));

        let next_pos = match curr_idx {
            Some(i) => {
                if self.shuffle {
                    use std::time::SystemTime;
                    let nanos = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .map(|d| d.subsec_nanos() as usize)
                        .unwrap_or(0);
                    (i + 1 + nanos % (tracks.len().saturating_sub(1).max(1))) % tracks.len()
                } else {
                    (i + 1) % tracks.len()
                }
            }
            None => tracks
                .iter()
                .position(|(idx, _)| *idx >= self.selected)
                .unwrap_or(0),
        };

        let (orig_idx, item) = tracks[next_pos];
        let item_cloned = item.clone();
        self.selected = orig_idx;
        self.play_item(&item_cloned);
    }

    pub fn play_prev(&mut self) {
        if self.position_ms() > 3000 {
            self.seek_to(0);
            return;
        }

        let items = self.cur_items().to_vec();
        let tracks: Vec<(usize, &LibItem)> = items
            .iter()
            .enumerate()
            .filter(|(_, i)| i.is_track && !i.is_header)
            .collect();
        if tracks.is_empty() {
            return;
        }

        let curr_uri = self.now.as_ref().map(|n| n.uri.as_str());
        let curr_idx = curr_uri.and_then(|uri| tracks.iter().position(|(_, item)| item.uri == uri));

        let prev_pos = match curr_idx {
            Some(i) => {
                if i > 0 {
                    i - 1
                } else {
                    tracks.len() - 1
                }
            }
            None => 0,
        };

        let (orig_idx, item) = tracks[prev_pos];
        let item_cloned = item.clone();
        self.selected = orig_idx;
        self.play_item(&item_cloned);
    }
}

pub fn context_target(item: &LibItem) -> Option<(String, String)> {
    (!item.is_header && !item.is_track).then(|| (item.uri.clone(), item.name.clone()))
}

pub fn enter_label(item: Option<&LibItem>) -> &'static str {
    match item {
        Some(i) if !i.is_track && !i.is_header => "open",
        _ => "select",
    }
}

pub fn play_selected_context(app: &mut App, shuffle: bool) {
    let Some(item) = app.cur_items().get(app.selected).cloned() else {
        return;
    };
    match context_target(&item) {
        Some((uri, name)) => app.play_context_row(uri, name, shuffle),
        None => app.status = "not a playlist, album, or artist".to_string(),
    }
}
