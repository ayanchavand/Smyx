//! smyx — a lean, beautiful terminal Navidrome / OpenSubsonic player.

use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{
    self, Event, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use smyx::app::App;
use smyx::config::NavidromeConfig;
use smyx::cover::Cover;
use smyx::engine::{Engine, EngineEvent};
use smyx::events::{advance_fade, handle_engine_event, handle_key, save_state};
use smyx::login_modal::LoginModalState;
use smyx::models::{LibItem, SavedState, Section, TrackMeta};
use smyx::subsonic::SubsonicClient;
use smyx::tasks::{liblog, spawn_library_fetch};
use smyx::theme::TOKYONIGHT;
use smyx::ui::{render, Term};

fn acquire_single_instance_lock() -> std::fs::File {
    use fs2::FileExt;
    let path = smyx::home_dir()
        .map(|h| h.join(".cache/smyx/lock"))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/smyx.lock"));
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .expect("open lock file");
    if file.try_lock_exclusive().is_err() {
        eprintln!("smyx is already running (another instance holds the lock).");
        eprintln!(
            "Close it first, or remove {} if it's stale.",
            path.display()
        );
        std::process::exit(1);
    }
    file
}

fn init_terminal() -> Result<Term> {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let mut out = io::stdout();
        let _ = execute!(
            out,
            crossterm::event::DisableMouseCapture,
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
        let _ = disable_raw_mode();
        default_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let _ = execute!(
        stdout,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore_terminal(terminal: &mut Term) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        crossterm::event::DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    let _ = execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags);
    terminal.show_cursor()?;
    Ok(())
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<()> {
    let _lock = acquire_single_instance_lock();

    let (engine_tx, engine_rx) = flume::unbounded::<EngineEvent>();
    let (lib_tx, lib_rx) = flume::unbounded::<(Section, Vec<LibItem>)>();
    let (queue_tx, _queue_rx) = flume::unbounded::<Vec<(String, String)>>();
    let (search_tx, search_rx) = flume::unbounded::<Vec<LibItem>>();
    let (detail_tx, detail_rx) = flume::unbounded::<(String, String, Vec<LibItem>)>();
    let (libdone_tx, libdone_rx) = flume::unbounded::<bool>();
    let (meta_tx, meta_rx) = flume::unbounded::<TrackMeta>();
    let (lyrics_tx, lyrics_rx) = flume::unbounded::<(Vec<(u32, String)>, bool)>();
    let (login_tx, login_rx) = flume::unbounded::<Result<(SubsonicClient, NavidromeConfig), String>>();
    let (sel_cover_tx, sel_cover_rx) = flume::unbounded::<(String, image::DynamicImage)>();
    let (update_tx, update_rx) = flume::unbounded::<String>();

    let update_tx_c = update_tx.clone();
    tokio::task::spawn_blocking(move || {
        use update_informer::{registry, Check};
        let informer = update_informer::new(
            registry::Crates,
            "smyx",
            env!("CARGO_PKG_VERSION"),
        )
        .interval(std::time::Duration::from_secs(86400));
        if let Ok(Some(new_ver)) = informer.check_version() {
            let _ = update_tx_c.send(new_ver.to_string());
        }
    });

    let config_opt = NavidromeConfig::load();
    let initial_client = config_opt.as_ref().map(|c| SubsonicClient::new(c.clone()));

    if let Some(ref client) = initial_client {
        let client_c = client.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = client_c.ping() {
                liblog(format!("Startup ping failed: {e:#}"));
            }
        });
    }

    let shared_subsonic = Arc::new(Mutex::new(initial_client.clone()));
    let dummy_client = initial_client.unwrap_or_else(|| {
        SubsonicClient::new(NavidromeConfig::new(
            "http://localhost:4533".to_string(),
            String::new(),
            String::new(),
        ))
    });

    let engine = Engine::new(dummy_client, engine_tx).context("failed to start audio engine")?;
    let state = SavedState::load();
    let mut terminal = init_terminal()?;
    let picker = Cover::make_picker();

    let login_modal = if config_opt.is_none() {
        let default_cfg = NavidromeConfig::new(
            "http://localhost:4533".to_string(),
            String::new(),
            String::new(),
        );
        Some(LoginModalState::from_config(&default_cfg))
    } else {
        None
    };

    let mut app = App::new(
        engine,
        picker,
        TOKYONIGHT,
        shared_subsonic.clone(),
        login_modal,
        state.volume,
    );

    app.shuffle = state.shuffle;
    app.repeat = state.repeat;
    app.queue = state.queue;
    app.queue_uris = state.queue_uris;
    app.source = state.source;
    app.source_name = state.source_name;

    if shared_subsonic.lock().unwrap().is_some() {
        app.status = "loading library…".to_string();
        spawn_library_fetch(app.subsonic.clone(), lib_tx.clone(), libdone_tx.clone());
    }

    let res = run_ui(
        &mut terminal,
        &mut app,
        engine_rx,
        lib_tx,
        lib_rx,
        queue_tx,
        search_tx,
        search_rx,
        detail_tx,
        detail_rx,
        libdone_tx,
        libdone_rx,
        meta_tx,
        meta_rx,
        lyrics_tx,
        lyrics_rx,
        login_tx,
        login_rx,
        sel_cover_tx,
        sel_cover_rx,
        update_rx,
    )
    .await;

    save_state(&app);
    restore_terminal(&mut terminal)?;
    res
}

#[allow(clippy::too_many_arguments)]
async fn run_ui(
    terminal: &mut Term,
    app: &mut App,
    engine_rx: flume::Receiver<EngineEvent>,
    lib_tx: flume::Sender<(Section, Vec<LibItem>)>,
    lib_rx: flume::Receiver<(Section, Vec<LibItem>)>,
    queue_tx: flume::Sender<Vec<(String, String)>>,
    search_tx: flume::Sender<Vec<LibItem>>,
    search_rx: flume::Receiver<Vec<LibItem>>,
    detail_tx: flume::Sender<(String, String, Vec<LibItem>)>,
    detail_rx: flume::Receiver<(String, String, Vec<LibItem>)>,
    libdone_tx: flume::Sender<bool>,
    libdone_rx: flume::Receiver<bool>,
    meta_tx: flume::Sender<TrackMeta>,
    meta_rx: flume::Receiver<TrackMeta>,
    lyrics_tx: flume::Sender<(Vec<(u32, String)>, bool)>,
    lyrics_rx: flume::Receiver<(Vec<(u32, String)>, bool)>,
    login_tx: flume::Sender<Result<(SubsonicClient, NavidromeConfig), String>>,
    login_rx: flume::Receiver<Result<(SubsonicClient, NavidromeConfig), String>>,
    sel_cover_tx: flume::Sender<(String, image::DynamicImage)>,
    sel_cover_rx: flume::Receiver<(String, image::DynamicImage)>,
    update_rx: flume::Receiver<String>,
) -> Result<()> {
    let mut ticker = tokio::time::interval(Duration::from_millis(33));

    loop {
        ticker.tick().await;

        while let Ok(ev) = engine_rx.try_recv() {
            handle_engine_event(app, ev, &meta_tx);
        }

        while let Ok((sec, items)) = lib_rx.try_recv() {
            app.library.set(sec, items);
            app.normalize_selection();
        }

        while let Ok(results) = search_rx.try_recv() {
            app.search_results = results;
            app.searching = true;
            app.status.clear();
            app.normalize_selection();
        }

        while let Ok((_uri, title, items)) = detail_rx.try_recv() {
            let parent_sel = app.selected;
            app.details.push(smyx::models::Detail {
                title,
                items,
                parent_selected: parent_sel,
            });
            app.selected = app.first_selectable();
        }

        while let Ok(ok) = libdone_rx.try_recv() {
            app.status = if ok {
                String::new()
            } else {
                "library empty or fetch failed".to_string()
            };
        }

        while let Ok(meta) = meta_rx.try_recv() {
            smyx::events::apply_meta(app, meta, &lyrics_tx);
        }

        while let Ok((lines, synced)) = lyrics_rx.try_recv() {
            app.lyrics = lines;
            app.lyrics_synced = synced;
        }

        while let Ok(result) = login_rx.try_recv() {
            if let Some(ref mut modal) = app.login_modal {
                modal.is_connecting = false;
            }
            match result {
                Ok((client, config)) => {
                    let _ = config.save();
                    app.engine.client = client.clone();
                    *app.subsonic.lock().unwrap() = Some(client);
                    app.login_modal = None;
                    app.status = "Logged in! Loading library…".to_string();
                    spawn_library_fetch(app.subsonic.clone(), lib_tx.clone(), libdone_tx.clone());
                }
                Err(err_msg) => {
                    if let Some(ref mut modal) = app.login_modal {
                        modal.error_message = Some(err_msg);
                    }
                }
            }
        }

        while let Ok((uri, img)) = sel_cover_rx.try_recv() {
            let cover = smyx::cover::Cover::from_image(img, app.picker.clone());
            app.selected_cover = Some((uri, cover));
        }

        while let Ok(new_ver) = update_rx.try_recv() {
            app.new_version = Some(new_ver);
        }

        if app.now.is_none() {
            if let Some(item) = app.cur_items().get(app.selected).cloned() {
                if item.is_track {
                    if app.selected_cover_uri.as_deref() != Some(&item.uri) {
                        app.selected_cover_uri = Some(item.uri.clone());
                        if let Some(id) = item.uri.strip_prefix("subsonic:track:") {
                            let subsonic = app.subsonic.clone();
                            let tx = sel_cover_tx.clone();
                            let uri = item.uri.clone();
                            let track_id = id.to_string();
                            tokio::task::spawn_blocking(move || {
                                let client_opt = subsonic.lock().unwrap().clone();
                                if let Some(client) = client_opt {
                                    let mut cover_id = track_id.clone();
                                    if let Ok(song) = client.get_song(&track_id) {
                                        if let Some(c) = song.cover_art {
                                            cover_id = c;
                                        }
                                    }
                                    if let Ok(cover_bytes) = client.get_cover_art(&cover_id) {
                                        if let Ok(img) = image::load_from_memory(&cover_bytes) {
                                            let _ = tx.send((uri, img));
                                        }
                                    }
                                }
                            });
                        }
                    }
                }
            }
        }

        advance_fade(app);

        terminal.draw(|f| render(f, app))?;

        while event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == crossterm::event::KeyEventKind::Press {
                    if handle_key(
                        app, key, &lib_tx, &queue_tx, &search_tx, &detail_tx, &libdone_tx, &login_tx,
                    ) {
                        return Ok(());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use smyx::app::{context_target, enter_label};
    use smyx::models::LibItem;
    use smyx::tasks::{parse_lrc, parse_lrc_stamp, urlencode};

    #[test]
    fn test_context_target() {
        let ctx = LibItem::ctx("Playlist".into(), "sub".into(), "subsonic:playlist:1".into());
        assert_eq!(context_target(&ctx), Some(("subsonic:playlist:1".into(), "Playlist".into())));

        let track = LibItem::track("Song".into(), "Artist".into(), "subsonic:track:9".into());
        assert!(context_target(&track).is_none());

        let header = LibItem::header("Section Header");
        assert!(context_target(&header).is_none());
    }


    #[test]
    fn test_enter_label() {
        let ctx = LibItem::ctx("Playlist".into(), "sub".into(), "subsonic:playlist:1".into());
        assert_eq!(enter_label(Some(&ctx)), "open");

        let track = LibItem::track("Song".into(), "Artist".into(), "subsonic:track:9".into());
        assert_eq!(enter_label(Some(&track)), "select");
    }

    #[test]
    fn test_parse_lrc_stamp() {
        assert_eq!(parse_lrc_stamp("01:23.45"), Some(83450));
        assert_eq!(parse_lrc_stamp("00:10"), Some(10000));
        assert_eq!(parse_lrc_stamp("invalid"), None);
    }

    #[test]
    fn test_parse_lrc() {
        let lrc = "[00:10.00]First line\n[00:20.00]Second line";
        let parsed = parse_lrc(lrc);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], (10000, "First line".to_string()));
        assert_eq!(parsed[1], (20000, "Second line".to_string()));
    }

    #[test]
    fn test_urlencode() {
        assert_eq!(urlencode("hello world"), "hello%20world");
        assert_eq!(urlencode("abc_123"), "abc_123");
    }
}
