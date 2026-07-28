//! Keyboard, mouse, and engine event processing for Smyx.

use std::time::{Duration, Instant};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MediaKeyCode};
use anyhow::Result;

use crate::app::{play_selected_context, App};
use crate::config::NavidromeConfig;
use crate::cover::Cover;
use crate::engine::EngineEvent;
use crate::login_modal::{LoginModalAction, LoginModalState};
use crate::models::{
    ActionItem, ActionKind, ActionMenu, Activated, LastPlayed, LibItem,
    NowPlaying, SavedState, Section, TrackMeta, ART_REPAINTS,
};
use crate::reactive::derive_theme;
use crate::subsonic::SubsonicClient;
use crate::tasks::{fetch_lyrics_blocking, spawn_detail_fetch, spawn_library_fetch, spawn_search};

pub fn handle_key(
    app: &mut App,
    key: KeyEvent,
    lib_tx: &flume::Sender<(Section, Vec<LibItem>)>,
    _queue_tx: &flume::Sender<Vec<(String, String)>>,
    search_tx: &flume::Sender<Vec<LibItem>>,
    detail_tx: &flume::Sender<(String, String, Vec<LibItem>)>,
    libdone_tx: &flume::Sender<bool>,
    login_tx: &flume::Sender<Result<(SubsonicClient, NavidromeConfig), String>>,
) -> bool {
    let code = key.code;
    let mods = key.modifiers;

    // Double-press Ctrl-C to quit (works from anywhere). Single press arms it.
    if code == KeyCode::Char('c') && mods.contains(KeyModifiers::CONTROL) {
        let now = Instant::now();
        if app
            .last_ctrl_c
            .map(|t| now.duration_since(t) < Duration::from_millis(1500))
            .unwrap_or(false)
        {
            return true;
        }
        app.last_ctrl_c = Some(now);
        app.status = "press Ctrl-C again to quit".to_string();
        return false;
    }

    // --- Login modal captures input while open ---
    if let Some(ref mut modal) = app.login_modal {
        if code == KeyCode::Esc && app.subsonic.lock().unwrap().is_some() {
            app.login_modal = None;
            return false;
        }
        let action = modal.handle_key_event(key);
        if let LoginModalAction::Submit(config) = action {
            modal.is_connecting = true;
            modal.error_message = None;
            let login_tx = login_tx.clone();
            tokio::task::spawn_blocking(move || {
                let client = SubsonicClient::new(config.clone());
                match client.ping() {
                    Ok(()) => {
                        let _ = login_tx.send(Ok((client, config)));
                    }
                    Err(e) => {
                        let _ = login_tx.send(Err(format!("Login failed: {e:#}")));
                    }
                }
            });
        }
        return false;
    }

    // --- Actions menu captures input while open ---
    if app.actions.is_some() {
        handle_action_key(app, code, lib_tx, libdone_tx, detail_tx);
        return false;
    }

    // --- Search input mode captures everything ---
    if app.input_mode {
        match code {
            KeyCode::Esc => app.input_mode = false,
            KeyCode::Enter => {
                app.input_mode = false;
                let q = app.query.trim().to_string();
                if !q.is_empty() {
                    app.searching = true;
                    app.selected = 0;
                    app.status = "searching…".to_string();
                    spawn_search(app.subsonic.clone(), q, search_tx.clone());
                }
            }
            KeyCode::Backspace => {
                app.query.pop();
            }
            KeyCode::Char(c) => app.query.push(c),
            _ => {}
        }
        return false;
    }

    match code {
        KeyCode::Char('/') => {
            app.input_mode = true;
            app.query.clear();
        }
        KeyCode::Char('L') => {
            if app.login_modal.is_none() {
                let cfg = NavidromeConfig::load().unwrap_or_else(|| NavidromeConfig::new(
                    "http://localhost:4533".to_string(),
                    String::new(),
                    String::new(),
                ));
                app.login_modal = Some(LoginModalState::from_config(&cfg));
            } else if app.subsonic.lock().unwrap().is_some() {
                app.login_modal = None;
            }
        }
        KeyCode::Char('q') => return true,
        KeyCode::Esc => {
            if let Some(d) = app.details.pop() {
                app.selected = d.parent_selected;
            } else if app.searching {
                app.searching = false;
                app.selected = 0;
            }
        }
        KeyCode::Char(' ') | KeyCode::Char('p') | KeyCode::Media(MediaKeyCode::PlayPause) => {
            app.engine.toggle_play();
        }
        KeyCode::Media(MediaKeyCode::Stop) => {
            app.engine.stop();
        }
        KeyCode::Char('n') | KeyCode::Media(MediaKeyCode::TrackNext) => {
            app.play_next();
        }
        KeyCode::Char('b') | KeyCode::Media(MediaKeyCode::TrackPrevious) => {
            app.play_prev();
        }
        KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Media(MediaKeyCode::RaiseVolume) => {
            app.volume = (app.volume + 5).min(100);
            app.engine.set_volume_u8(app.volume);
        }
        KeyCode::Char('-') | KeyCode::Char('_') | KeyCode::Media(MediaKeyCode::LowerVolume) => {
            app.volume = app.volume.saturating_sub(5);
            app.engine.set_volume_u8(app.volume);
        }
        KeyCode::Char('s') => {
            app.shuffle = !app.shuffle;
            app.engine.shuffle(app.shuffle);
        }
        KeyCode::Char('P') => play_selected_context(app, false),
        KeyCode::Char('S') => {
            app.shuffle = true;
            app.engine.shuffle(true);
            play_selected_context(app, true);
        }
        KeyCode::Char('R') => {
            app.repeat = !app.repeat;
            app.engine.repeat(app.repeat);
        }
        KeyCode::Char('r') => {
            app.status = "loading library…".to_string();
            spawn_library_fetch(app.subsonic.clone(), lib_tx.clone(), libdone_tx.clone());
        }
        KeyCode::Char('o') => {
            app.sort = app.sort.next();
            let m = app.sort;
            crate::models::sort_list(app.cur_list_mut(), m);
            app.selected = app.first_selectable();
            app.status = format!("sorted by {}", m.label());
        }
        KeyCode::Char('a') => {
            open_action_menu(app);
        }
        KeyCode::Char('l') => {
            if let Some(item) = app.cur_items().get(app.selected).cloned() {
                if item.is_track {
                    if let Some(id) = item.uri.strip_prefix("subsonic:track:") {
                        let id = id.to_string();
                        if let Some(subsonic) = app.subsonic.lock().unwrap().clone() {
                            let is_liked = app.library.liked.iter().any(|i| i.uri == item.uri);
                            let lib_tx = lib_tx.clone();
                            let libdone_tx = libdone_tx.clone();
                            let client_arc = app.subsonic.clone();
                            if is_liked {
                                app.status = format!("unliking {}…", item.name);
                                std::thread::spawn(move || {
                                    let _ = subsonic.unstar(&id);
                                    spawn_library_fetch(client_arc, lib_tx, libdone_tx);
                                });
                            } else {
                                app.status = format!("liking {}…", item.name);
                                std::thread::spawn(move || {
                                    let _ = subsonic.star(&id);
                                    spawn_library_fetch(client_arc, lib_tx, libdone_tx);
                                });
                            }
                        }
                    }
                }
            }
        }
        KeyCode::Char('t') => {
            let cur_name = app.target.name;
            let idx = crate::theme::THEMES.iter().position(|t| t.name == cur_name).unwrap_or(0);
            let next = crate::theme::THEMES[(idx + 1) % crate::theme::THEMES.len()];
            app.status = format!("theme: {}", next.name);
            app.start_fade(next);
        }
        KeyCode::Tab | KeyCode::Char(']') => {
            app.searching = false;
            app.section = app.section.shift(1);
            app.selected = app.first_selectable();
        }
        KeyCode::BackTab => {
            app.view = app.view.shift(1);
        }
        KeyCode::Char('[') => {
            app.searching = false;
            app.section = app.section.shift(-1);
            app.selected = app.first_selectable();
        }
        KeyCode::Right => app.seek_by(5_000),
        KeyCode::Left => app.seek_by(-5_000),
        KeyCode::Down | KeyCode::Char('j') => app.move_sel(1),
        KeyCode::Up | KeyCode::Char('k') => app.move_sel(-1),
        KeyCode::Enter => match app.activate() {
            Activated::Open(uri, name) => {
                spawn_detail_fetch(app.subsonic.clone(), uri, name, detail_tx.clone());
            }
            Activated::None => {}
        },
        _ => {}
    }
    false
}

pub fn handle_action_key(
    app: &mut App,
    code: KeyCode,
    lib_tx: &flume::Sender<(Section, Vec<LibItem>)>,
    libdone_tx: &flume::Sender<bool>,
    detail_tx: &flume::Sender<(String, String, Vec<LibItem>)>,
) {
    match code {
        KeyCode::Esc | KeyCode::Char('a') => {
            app.actions = None;
            app.status.clear();
            return;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(m) = app.actions.as_mut() {
                m.selected = m.selected.saturating_sub(1);
            }
            return;
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(m) = app.actions.as_mut() {
                m.selected = (m.selected + 1).min(m.items.len().saturating_sub(1));
            }
            return;
        }
        KeyCode::Enter => {}
        _ => return,
    }

    let kind = app
        .actions
        .as_ref()
        .and_then(|m| m.items.get(m.selected))
        .map(|i| i.kind.clone());
    let Some(kind) = kind else { return };
    match kind {
        ActionKind::AddToPlaylistMenu { track_uri } => {
            let items: Vec<ActionItem> = app
                .library
                .playlists
                .iter()
                .filter_map(|p| {
                    let id = p.uri.rsplit(':').next()?.to_string();
                    Some(ActionItem {
                        label: p.name.clone(),
                        kind: ActionKind::AddToPlaylist {
                            playlist_id: id,
                            track_uri: track_uri.clone(),
                        },
                    })
                })
                .collect();
            if items.is_empty() {
                app.status = "no playlists to add to".to_string();
                app.actions = None;
            } else {
                app.actions = Some(ActionMenu {
                    title: "Add to playlist".to_string(),
                    items,
                    selected: 0,
                });
            }
        }
        ActionKind::Play { uri, name } => {
            let shuffle = app.shuffle;
            app.play_context_row(uri, name, shuffle);
            app.actions = None;
        }
        ActionKind::Open { uri, name } => {
            spawn_detail_fetch(app.subsonic.clone(), uri, name, detail_tx.clone());
            app.actions = None;
        }
        ActionKind::CopyLink { uri } => {
            app.status = if copy_to_clipboard(&uri) {
                "link copied".to_string()
            } else {
                "clipboard unavailable".to_string()
            };
            app.actions = None;
        }
        ActionKind::Queue { uri } => {
            let label = app
                .cur_items()
                .iter()
                .find(|i| i.uri == uri)
                .map(|i| i.name.clone())
                .unwrap_or_else(|| uri.clone());
            app.queue.push(label.clone());
            app.queue_uris.push(uri);
            app.status = format!("queued: {}", label);
            app.actions = None;
        }
        ActionKind::PlayNext { uri } => {
            let label = app
                .cur_items()
                .iter()
                .find(|i| i.uri == uri)
                .map(|i| i.name.clone())
                .unwrap_or_else(|| uri.clone());
            app.queue.insert(0, label.clone());
            app.queue_uris.insert(0, uri);
            app.status = format!("queued next: {}", label);
            app.actions = None;
        }
        ActionKind::AddToPlaylist { playlist_id, track_uri } => {
            let song_id = track_uri.strip_prefix("subsonic:track:").unwrap_or(&track_uri).to_string();
            if let Some(subsonic) = app.subsonic.lock().unwrap().clone() {
                let pid = playlist_id.clone();
                let sid = song_id.clone();
                std::thread::spawn(move || {
                    let _ = subsonic.add_to_playlist(&pid, &sid);
                });
                app.status = "added track to playlist".to_string();
            } else {
                app.status = "not connected".to_string();
            }
            app.actions = None;
        }
        ActionKind::StarTrack { track_uri } => {
            let song_id = track_uri.strip_prefix("subsonic:track:").unwrap_or(&track_uri).to_string();
            if let Some(subsonic) = app.subsonic.lock().unwrap().clone() {
                let sid = song_id.clone();
                let lib_tx = lib_tx.clone();
                let libdone_tx = libdone_tx.clone();
                let client_arc = app.subsonic.clone();
                std::thread::spawn(move || {
                    let _ = subsonic.star(&sid);
                    spawn_library_fetch(client_arc, lib_tx, libdone_tx);
                });
                app.status = "liked track".to_string();
            } else {
                app.status = "not connected".to_string();
            }
            app.actions = None;
        }
        ActionKind::UnstarTrack { track_uri } => {
            let song_id = track_uri.strip_prefix("subsonic:track:").unwrap_or(&track_uri).to_string();
            if let Some(subsonic) = app.subsonic.lock().unwrap().clone() {
                let sid = song_id.clone();
                let lib_tx = lib_tx.clone();
                let libdone_tx = libdone_tx.clone();
                let client_arc = app.subsonic.clone();
                std::thread::spawn(move || {
                    let _ = subsonic.unstar(&sid);
                    spawn_library_fetch(client_arc, lib_tx, libdone_tx);
                });
                app.status = "unliked track".to_string();
            } else {
                app.status = "not connected".to_string();
            }
            app.actions = None;
        }
        _other => {
            app.actions = None;
        }
    }
}

pub fn open_action_menu(app: &mut App) {
    let Some(item) = app.cur_items().get(app.selected).cloned() else {
        return;
    };
    if item.is_header {
        return;
    }

    let mut items = Vec::new();

    if item.is_track {
        let is_liked = app.library.liked.iter().any(|i| i.uri == item.uri);
        if is_liked {
            items.push(ActionItem {
                label: "Unlike track".to_string(),
                kind: ActionKind::UnstarTrack {
                    track_uri: item.uri.clone(),
                },
            });
        } else {
            items.push(ActionItem {
                label: "Like track".to_string(),
                kind: ActionKind::StarTrack {
                    track_uri: item.uri.clone(),
                },
            });
        }
        items.push(ActionItem {
            label: "Add to queue".to_string(),
            kind: ActionKind::Queue {
                uri: item.uri.clone(),
            },
        });
        items.push(ActionItem {
            label: "Add to playlist...".to_string(),
            kind: ActionKind::AddToPlaylistMenu {
                track_uri: item.uri.clone(),
            },
        });
        items.push(ActionItem {
            label: format!("Play {}", item.name),
            kind: ActionKind::Play {
                uri: item.uri.clone(),
                name: item.name.clone(),
            },
        });
    } else {
        items.push(ActionItem {
            label: format!("Open {}", item.name),
            kind: ActionKind::Open {
                uri: item.uri.clone(),
                name: item.name.clone(),
            },
        });
        items.push(ActionItem {
            label: format!("Play {}", item.name),
            kind: ActionKind::Play {
                uri: item.uri.clone(),
                name: item.name.clone(),
            },
        });
    }

    items.push(ActionItem {
        label: "Copy Link".to_string(),
        kind: ActionKind::CopyLink {
            uri: item.uri.clone(),
        },
    });

    app.actions = Some(ActionMenu {
        title: item.name.clone(),
        items,
        selected: 0,
    });
}

pub fn copy_to_clipboard(text: &str) -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let candidates: [(&str, &[&str]); 4] = [
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["-b", "-i"]),
        ("pbcopy", &[]),
    ];
    for (cmd, args) in candidates {
        if let Ok(mut child) = Command::new(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(mut sin) = child.stdin.take() {
                let _ = sin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            return true;
        }
    }
    false
}

pub fn save_state(app: &App) {
    let last_played = app.now.as_ref().map(|now| LastPlayed {
        uri: now.uri.clone(),
        title: now.title.clone(),
        artist: now.artist.clone(),
        album: now.album.clone(),
        duration_ms: now.duration_ms,
        position_ms: app.position_ms(),
    });

    let s = SavedState {
        volume: app.volume,
        shuffle: app.shuffle,
        repeat: app.repeat,
        last_played,
        queue: app.queue.clone(),
        queue_uris: app.queue_uris.clone(),
        source: app.source.clone(),
        source_name: app.source_name.clone(),
    };
    s.save();
}

pub fn advance_fade(app: &mut App) {
    if let Some(fade) = &app.fade {
        app.displayed = fade.current();
        if fade.is_done() {
            app.displayed = app.target;
            app.fade = None;
        }
    }
}

pub fn handle_engine_event(app: &mut App, ev: EngineEvent, meta_tx: &flume::Sender<TrackMeta>) {
    match ev {
        EngineEvent::TrackChanged { uri } => {
            app.status = "loading track…".to_string();
            if let Some(track_id) = uri.strip_prefix("subsonic:track:") {
                app.pending_meta = Some(format!("subsonic:track:{track_id}"));
                let subsonic = app.subsonic.clone();
                let tx = meta_tx.clone();
                let track_id = track_id.to_string();
                tokio::task::spawn_blocking(move || {
                    let client_opt = subsonic.lock().unwrap().clone();
                    if let Some(client) = client_opt {
                        let mut title = String::new();
                        let mut artist = String::new();
                        let mut album = String::new();
                        let mut duration_ms = 0;
                        let mut cover_id = track_id.clone();

                        if let Ok(song) = client.get_song(&track_id) {
                            title = song.display_title();
                            artist = song.artist.unwrap_or_default();
                            album = song.album.unwrap_or_default();
                            duration_ms = song.duration.map(|d| d * 1000).unwrap_or(0);
                            if let Some(c) = song.cover_art {
                                cover_id = c;
                            }
                        }

                        let cover_bytes = client.get_cover_art(&cover_id).ok();
                        let img = cover_bytes.and_then(|b| image::load_from_memory(&b).ok());
                        let theme = img.as_ref().map(|i| derive_theme(i, "album ✦"));
                        let _ = tx.send(TrackMeta {
                            uri: format!("subsonic:track:{}", track_id),
                            title,
                            artist,
                            album,
                            duration_ms,
                            image: img,
                            theme,
                        });
                    }
                });
            }
        }
        EngineEvent::Playing { position_ms, .. } => {
            if !app.playback_started {
                app.playback_started = true;
                let _ = app.engine.shuffle(app.shuffle);
                let _ = app.engine.repeat(app.repeat);
                let _ = app.engine.set_volume_u8(app.volume);
            }
            if let Some(n) = app.now.as_mut() {
                n.is_playing = true;
                n.position_ms = position_ms;
                n.position_at = Instant::now();
            }
        }
        EngineEvent::Paused { position_ms, .. } => {
            if let Some(n) = app.now.as_mut() {
                n.is_playing = false;
                n.position_ms = position_ms;
                n.position_at = Instant::now();
            }
        }
        EngineEvent::Stopped => {
            app.now = None;
            app.playback_started = false;
        }
        EngineEvent::PositionCorrection { position_ms, .. } => {
            if let Some(n) = app.now.as_mut() {
                n.position_ms = position_ms;
                n.position_at = Instant::now();
            }
        }
        EngineEvent::EndOfTrack { uri } => {
            if app.now.as_ref().is_some_and(|n| n.uri == uri) {
                app.play_next();
            }
        }
    }
}

pub fn meta_is_current(pending: Option<&str>, meta_uri: &str) -> bool {
    pending.is_none_or(|p| p == meta_uri)
}

pub fn apply_meta(
    app: &mut App,
    meta: TrackMeta,
    lyrics_tx: &flume::Sender<(Vec<(u32, String)>, bool)>,
) {
    if !meta_is_current(app.pending_meta.as_deref(), &meta.uri) {
        return;
    }

    let cover = meta
        .image
        .as_ref()
        .map(|img| Cover::from_image(img.clone(), app.picker.clone()));
    app.art_dirty = ART_REPAINTS;
    app.status.clear();
    app.lyrics.clear();
    app.lyrics_synced = false;

    if !meta.title.is_empty() {
        let (artist, title, album, dur) = (
            meta.artist.clone(),
            meta.title.clone(),
            meta.album.clone(),
            meta.duration_ms,
        );
        let tx = lyrics_tx.clone();
        tokio::task::spawn_blocking(move || {
            let _ = tx.send(fetch_lyrics_blocking(&artist, &title, &album, dur));
        });
    }

    app.now = Some(NowPlaying {
        uri: meta.uri,
        title: meta.title,
        artist: meta.artist,
        album: meta.album,
        duration_ms: meta.duration_ms,
        position_ms: app.now.as_ref().map(|n| n.position_ms).unwrap_or(0),
        position_at: Instant::now(),
        is_playing: app
            .now
            .as_ref()
            .map(|n| n.is_playing)
            .unwrap_or(app.playback_started),
        cover,
    });
    if let Some(theme) = meta.theme {
        app.start_fade(theme);
    }
}
