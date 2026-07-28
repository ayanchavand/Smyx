//! Asynchronous background tasks and utility routines for Myx.

use std::sync::{Arc, Mutex};
use crate::models::{LibItem, Section};
use crate::subsonic::SubsonicClient;

/// Optional debug log — silent unless `MYX_LOG` is set. Writes to
/// ~/.cache/myx/myx.log instead of a fixed /tmp path.
pub fn liblog(msg: impl AsRef<str>) {
    use std::io::Write;
    if std::env::var_os("MYX_LOG").is_none() {
        return;
    }
    let Some(home) = crate::home_dir() else { return };
    let dir = home.join(".cache/myx");
    if std::fs::create_dir_all(&dir).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        }
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    if let Ok(mut f) = opts.open(dir.join("myx.log")) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        let _ = writeln!(f, "{ts:.3} {}", msg.as_ref());
    }
}

/// Fetch the library incrementally: fast sections first, Liked streamed in
/// chunks so the UI is usable within ~1s instead of waiting for everything.
pub fn spawn_library_fetch(
    subsonic: Arc<Mutex<Option<SubsonicClient>>>,
    tx: flume::Sender<(Section, Vec<LibItem>)>,
    done_tx: flume::Sender<bool>,
) {
    let client_opt = subsonic.lock().unwrap().clone();
    std::thread::Builder::new()
        .name("myx-library".to_string())
        .spawn(move || {
            let Some(client) = client_opt else {
                let _ = done_tx.send(false);
                return;
            };

            let mut got_any = false;

            // 1. Playlists
            if let Ok(playlists) = client.get_playlists() {
                got_any = true;
                let items: Vec<LibItem> = playlists
                    .into_iter()
                    .map(|p| {
                        LibItem::ctx(
                            p.name,
                            format!("{} tracks", p.song_count.unwrap_or(0)),
                            format!("subsonic:playlist:{}", p.id),
                        )
                    })
                    .collect();
                let _ = tx.send((Section::Playlists, items));
            }

            // 2. Liked (Starred)
            if let Ok((songs, _, _)) = client.get_starred() {
                got_any = true;
                let items: Vec<LibItem> = songs
                    .into_iter()
                    .map(|s| {
                        LibItem::track(
                            s.display_title(),
                            s.artist.unwrap_or_default(),
                            format!("subsonic:track:{}", s.id),
                        )
                    })
                    .collect();
                let _ = tx.send((Section::Home, items.clone()));
                let _ = tx.send((Section::Liked, items));
            }

            // 3. Albums
            if let Ok(albums) = client.get_album_list("alphabeticalByTitle", 100) {
                got_any = true;
                let items: Vec<LibItem> = albums
                    .into_iter()
                    .map(|a| {
                        LibItem::ctx(
                            a.name,
                            a.artist.unwrap_or_default(),
                            format!("subsonic:album:{}", a.id),
                        )
                    })
                    .collect();
                let _ = tx.send((Section::Albums, items));
            }

            // 4. Artists
            if let Ok(artists) = client.get_artists() {
                got_any = true;
                let items: Vec<LibItem> = artists
                    .into_iter()
                    .map(|a| {
                        LibItem::ctx(
                            a.name,
                            format!("{} albums", a.album_count.unwrap_or(0)),
                            format!("subsonic:artist:{}", a.id),
                        )
                    })
                    .collect();
                let _ = tx.send((Section::Artists, items));
            }

            let _ = done_tx.send(got_any);
        })
        .expect("spawn library worker");
}

/// Perform Subsonic search in background thread.
pub fn spawn_search(
    subsonic: Arc<Mutex<Option<SubsonicClient>>>,
    query: String,
    tx: flume::Sender<Vec<LibItem>>,
) {
    let client_opt = subsonic.lock().unwrap().clone();
    std::thread::Builder::new()
        .name("myx-search".to_string())
        .spawn(move || {
            let Some(client) = client_opt else { return };
            let mut results = Vec::new();
            if let Ok((songs, albums, artists)) = client.search(&query) {
                if !songs.is_empty() {
                    results.push(LibItem::header("Songs"));
                    for s in songs {
                        results.push(LibItem::track(
                            s.display_title(),
                            s.artist.unwrap_or_default(),
                            format!("subsonic:track:{}", s.id),
                        ));
                    }
                }
                if !albums.is_empty() {
                    results.push(LibItem::header("Albums"));
                    for a in albums {
                        results.push(LibItem::ctx(
                            a.name,
                            a.artist.unwrap_or_default(),
                            format!("subsonic:album:{}", a.id),
                        ));
                    }
                }
                if !artists.is_empty() {
                    results.push(LibItem::header("Artists"));
                    for a in artists {
                        results.push(LibItem::ctx(
                            a.name,
                            format!("{} albums", a.album_count.unwrap_or(0)),
                            format!("subsonic:artist:{}", a.id),
                        ));
                    }
                }
            }
            let _ = tx.send(results);
        })
        .ok();
}

/// Fetch lyrics from LrcLib synchronously (called inside async worker).
pub fn fetch_lyrics_blocking(
    artist: &str,
    title: &str,
    album: &str,
    duration_ms: u32,
) -> (Vec<(u32, String)>, bool) {
    let url = format!(
        "https://lrclib.net/api/get?artist_name={}&track_name={}&album_name={}&duration={}",
        urlencode(artist),
        urlencode(title),
        urlencode(album),
        duration_ms / 1000
    );
    let Ok(res) = ureq::get(&url)
        .header("User-Agent", "myx (terminal player)")
        .call()
    else {
        return (Vec::new(), false);
    };
    let status = res.status().as_u16();
    if status < 200 || status >= 300 {
        return (Vec::new(), false);
    }
    let Ok(v) = res.into_body().read_json::<serde_json::Value>() else {
        return (Vec::new(), false);
    };

    if let Some(synced) = v["syncedLyrics"].as_str().filter(|s| !s.is_empty()) {
        return (parse_lrc(synced), true);
    }
    if let Some(plain) = v["plainLyrics"].as_str().filter(|s| !s.is_empty()) {
        let lines = plain.lines().map(|l| (0u32, l.to_string())).collect();
        return (lines, false);
    }
    (Vec::new(), false)
}

/// Parse LRC `[mm:ss.xx] text` lines into sorted (ms, text) pairs.
pub fn parse_lrc(lrc: &str) -> Vec<(u32, String)> {
    let mut out: Vec<(u32, String)> = Vec::new();
    for line in lrc.lines() {
        let mut rest = line;
        let mut stamps: Vec<u32> = Vec::new();
        while rest.starts_with('[') {
            let Some(end) = rest.find(']') else { break };
            let tag = &rest[1..end];
            if let Some(ms) = parse_lrc_stamp(tag) {
                stamps.push(ms);
            }
            rest = rest[end + 1..].trim_start();
            if stamps.is_empty() {
                break;
            }
        }
        let text = rest.trim().to_string();
        for ms in stamps {
            out.push((ms, text.clone()));
        }
    }
    out.sort_by_key(|(t, _)| *t);
    out
}

pub fn parse_lrc_stamp(tag: &str) -> Option<u32> {
    let (mm, rest) = tag.split_once(':')?;
    let mm: u32 = mm.parse().ok()?;
    let (ss, cs) = match rest.split_once('.') {
        Some((s, c)) => (s.parse::<u32>().ok()?, c),
        None => (rest.parse::<u32>().ok()?, "0"),
    };
    let cs: u32 = format!("{cs:0<3}")[..3].parse().unwrap_or(0);
    Some((mm * 60 + ss) * 1000 + cs)
}

pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Drill-in detail (artist / album / playlist).
pub fn spawn_detail_fetch(
    subsonic: Arc<Mutex<Option<SubsonicClient>>>,
    uri: String,
    name: String,
    tx: flume::Sender<(String, String, Vec<LibItem>)>,
) {
    let client_opt = subsonic.lock().unwrap().clone();
    std::thread::Builder::new()
        .name("myx-detail".to_string())
        .spawn(move || {
            let Some(client) = client_opt else { return };
            let mut items = Vec::new();
            if let Some(id) = uri.strip_prefix("subsonic:playlist:") {
                if let Ok(songs) = client.get_playlist_tracks(id) {
                    items = songs
                        .into_iter()
                        .map(|s| {
                            let title = s.display_title();
                            LibItem::track(
                                title,
                                s.artist.unwrap_or_default(),
                                format!("subsonic:track:{}", s.id),
                            )
                        })
                        .collect();
                }
            } else if let Some(id) = uri.strip_prefix("subsonic:album:") {
                if let Ok(songs) = client.get_album_tracks(id) {
                    items = songs
                        .into_iter()
                        .map(|s| {
                            let title = s.display_title();
                            LibItem::track(
                                title,
                                s.artist.unwrap_or_default(),
                                format!("subsonic:track:{}", s.id),
                            )
                        })
                        .collect();
                }
            } else if let Some(id) = uri.strip_prefix("subsonic:artist:") {
                if let Ok(albums) = client.get_artist_albums(id) {
                    items = albums
                        .into_iter()
                        .map(|a| {
                            LibItem::ctx(
                                a.name,
                                a.artist.unwrap_or_default(),
                                format!("subsonic:album:{}", a.id),
                            )
                        })
                        .collect();
                }
            }
            let _ = tx.send((uri, name, items));
        })
        .ok();
}
