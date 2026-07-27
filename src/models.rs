//! Data models and domain types for Myx.

use std::path::PathBuf;
use std::time::Instant;
use serde::{Deserialize, Serialize};

use crate::cover::Cover;
use crate::theme::Theme;

pub const FADE_MS: u64 = 300;
pub const ART_REPAINTS: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightView {
    NowPlaying,
    Lyrics,
    Queue,
}

impl RightView {
    pub const ALL: [RightView; 3] = [RightView::NowPlaying, RightView::Lyrics, RightView::Queue];

    pub fn label(self) -> &'static str {
        match self {
            RightView::NowPlaying => "Now Playing",
            RightView::Lyrics => "Lyrics",
            RightView::Queue => "Queue",
        }
    }

    pub fn shift(self, delta: isize) -> RightView {
        let i = RightView::ALL.iter().position(|&v| v == self).unwrap_or(0) as isize;
        let n = RightView::ALL.len() as isize;
        RightView::ALL[(i + delta).rem_euclid(n) as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Home,
    Recent,
    Playlists,
    Liked,
    Albums,
    Artists,
}

impl Section {
    pub const ALL: [Section; 6] = [
        Section::Home,
        Section::Liked,
        Section::Playlists,
        Section::Albums,
        Section::Artists,
        Section::Recent,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Section::Home => "Home",
            Section::Recent => "Recent",
            Section::Playlists => "Playlists",
            Section::Liked => "Liked",
            Section::Albums => "Albums",
            Section::Artists => "Artists",
        }
    }

    pub fn index(self) -> usize {
        Section::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }

    pub fn shift(self, delta: isize) -> Section {
        let n = Section::ALL.len() as isize;
        let i = (self.index() as isize + delta).rem_euclid(n) as usize;
        Section::ALL[i]
    }
}

/// A library entry. Behavior on Enter is driven by the flags:
/// header = non-selectable label; track = play as a track list; play = play this
/// URI as a context; otherwise = open (drill into) this context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibItem {
    pub name: String,
    pub subtitle: String,
    pub uri: String,
    pub is_track: bool,
    pub is_header: bool,
    pub is_play: bool,
    pub order: u32, // original fetch position (for the "Added" sort)
}

impl LibItem {
    pub fn track(name: String, subtitle: String, uri: String) -> Self {
        Self {
            name,
            subtitle,
            uri,
            is_track: true,
            is_header: false,
            is_play: false,
            order: 0,
        }
    }

    pub fn ctx(name: String, subtitle: String, uri: String) -> Self {
        Self {
            name,
            subtitle,
            uri,
            is_track: false,
            is_header: false,
            is_play: false,
            order: 0,
        }
    }

    pub fn play(name: String, uri: String) -> Self {
        Self {
            name,
            subtitle: String::new(),
            uri,
            is_track: false,
            is_header: false,
            is_play: true,
            order: 0,
        }
    }

    pub fn header(name: &str) -> Self {
        Self {
            name: name.to_string(),
            subtitle: String::new(),
            uri: String::new(),
            is_track: false,
            is_header: true,
            is_play: false,
            order: 0,
        }
    }
}

/// Sort order for browsable lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Added,
    Title,
    Artist,
}

impl SortMode {
    pub fn label(self) -> &'static str {
        match self {
            SortMode::Added => "added",
            SortMode::Title => "title",
            SortMode::Artist => "artist",
        }
    }

    pub fn next(self) -> SortMode {
        match self {
            SortMode::Added => SortMode::Title,
            SortMode::Title => SortMode::Artist,
            SortMode::Artist => SortMode::Added,
        }
    }
}

/// Sort a list in place, keeping leading header/play rows pinned at the top.
pub fn sort_list(items: &mut [LibItem], mode: SortMode) {
    let pin = items
        .iter()
        .take_while(|i| i.is_header || i.is_play)
        .count();
    let tail = &mut items[pin..];
    match mode {
        SortMode::Added => tail.sort_by_key(|i| i.order),
        SortMode::Title => tail.sort_by_key(|i| i.name.to_lowercase()),
        SortMode::Artist => tail.sort_by_key(|i| i.subtitle.to_lowercase()),
    }
}

/// A drill-in detail view (artist / album / playlist contents).
pub struct Detail {
    pub title: String,
    pub items: Vec<LibItem>,
    pub parent_selected: usize,
}

/// What an action-menu entry does when activated.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum ActionKind {
    ToggleLike {
        id: String,
        saved: bool,
    },
    Queue {
        uri: String,
    },
    AddToPlaylistMenu {
        track_uri: String,
    },
    AddToPlaylist {
        playlist_id: String,
        track_uri: String,
    },
    ToggleFollowArtist {
        id: String,
        following: bool,
    },
    ToggleSaveAlbum {
        id: String,
        saved: bool,
    },
    FollowPlaylist {
        id: String,
    },
    Play {
        uri: String,
        /// Carried so the play path can set `source_name` — without it the
        /// Queue view's PLAYING FROM header and the persisted resume source
        /// go stale.
        name: String,
    },
    Open {
        uri: String,
        name: String,
    },
    CopyLink {
        uri: String,
    },
}

pub struct ActionItem {
    pub label: String,
    pub kind: ActionKind,
}

pub struct ActionMenu {
    pub title: String,
    pub items: Vec<ActionItem>,
    pub selected: usize,
}

/// Result of activating (Enter on) a library item.
pub enum Activated {
    None,
    Open(String, String), // drill into a context (uri, name)
}

#[derive(Default, Clone)]
pub struct Library {
    pub home: Vec<LibItem>,
    pub recent: Vec<LibItem>,
    pub playlists: Vec<LibItem>,
    pub liked: Vec<LibItem>,
    pub albums: Vec<LibItem>,
    pub artists: Vec<LibItem>,
}

impl Library {
    pub fn items(&self, s: Section) -> &[LibItem] {
        match s {
            Section::Home => &self.home,
            Section::Recent => &self.recent,
            Section::Playlists => &self.playlists,
            Section::Liked => &self.liked,
            Section::Albums => &self.albums,
            Section::Artists => &self.artists,
        }
    }

    pub fn items_mut(&mut self, s: Section) -> &mut Vec<LibItem> {
        match s {
            Section::Home => &mut self.home,
            Section::Recent => &mut self.recent,
            Section::Playlists => &mut self.playlists,
            Section::Liked => &mut self.liked,
            Section::Albums => &mut self.albums,
            Section::Artists => &mut self.artists,
        }
    }

    pub fn set(&mut self, s: Section, items: Vec<LibItem>) {
        match s {
            Section::Home => self.home = items,
            Section::Recent => self.recent = items,
            Section::Playlists => self.playlists = items,
            Section::Liked => self.liked = items,
            Section::Albums => self.albums = items,
            Section::Artists => self.artists = items,
        }
    }
}

pub struct NowPlaying {
    pub uri: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u32,
    pub position_ms: u32,
    pub position_at: Instant,
    pub is_playing: bool,
    pub cover: Option<Cover>,
}

pub struct TrackMeta {
    pub uri: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u32,
    pub image: Option<image::DynamicImage>,
    pub theme: Option<Theme>,
}

/// What kind of thing is currently playing — persisted so we can resume the real
/// context (and its live queue) on reboot, not just a bare track.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub enum PlaySource {
    #[default]
    None,
    Context(String), // playlist / album / artist URI
    Radio(String),   // seed track URI
    Liked,
}

/// Persisted across sessions (~/.cache/myx/state.json).
#[derive(Default, Serialize, Deserialize)]
pub struct SavedState {
    pub volume: u8,
    #[serde(default)]
    pub shuffle: bool,
    #[serde(default)]
    pub repeat: bool,
    #[serde(default)]
    pub last_played: Option<LastPlayed>,
    pub queue: Vec<String>,
    #[serde(default)]
    pub queue_uris: Vec<String>,
    #[serde(default)]
    pub source: PlaySource,
    #[serde(default)]
    pub source_name: String,
}

#[derive(Default, Serialize, Deserialize)]
pub struct LastPlayed {
    pub uri: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u32,
    pub position_ms: u32,
}

impl SavedState {
    pub fn path() -> Option<PathBuf> {
        Some(crate::home_dir()?.join(".cache/myx/state.json"))
    }

    pub fn load() -> SavedState {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string(self) {
            let _ = std::fs::write(path, json);
        }
    }
}
