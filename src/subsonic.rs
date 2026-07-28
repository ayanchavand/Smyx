//! OpenSubsonic / Navidrome REST client module.
//!
//! Connects to Navidrome server endpoints using plain password authentication (`u`, `p`).

use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::io::Read;
use std::time::Duration;

use crate::config::NavidromeConfig;

pub const SUBSONIC_VERSION: &str = "1.16.1";
pub const CLIENT_NAME: &str = "myx";

#[derive(Debug, Clone)]
pub struct SubsonicClient {
    pub server_url: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct SubsonicResponseWrapper<T> {
    #[serde(rename = "subsonic-response")]
    pub response: SubsonicResponse<T>,
}

#[derive(Debug, Deserialize)]
pub struct SubsonicResponse<T> {
    pub status: String,
    pub version: Option<String>,
    pub error: Option<SubsonicError>,
    #[serde(flatten)]
    pub data: T,
}

#[derive(Debug, Deserialize)]
pub struct SubsonicError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct PingData {}

// --- OpenSubsonic Data Models ---

#[derive(Debug, Deserialize, Clone)]
pub struct SubsonicSong {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration: Option<u32>,
    #[serde(rename = "coverArt")]
    pub cover_art: Option<String>,
}

impl SubsonicSong {
    pub fn display_title(&self) -> String {
        if let Some(ref t) = self.title {
            if !t.is_empty() {
                return t.clone();
            }
        }
        if let Some(ref n) = self.name {
            if !n.is_empty() {
                return n.clone();
            }
        }
        "Untitled Track".to_string()
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct SubsonicPlaylist {
    pub id: String,
    pub name: String,
    #[serde(rename = "songCount")]
    pub song_count: Option<u32>,
    pub duration: Option<u32>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SubsonicAlbum {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    pub artist: Option<String>,
    #[serde(rename = "songCount")]
    pub song_count: Option<u32>,
    #[serde(rename = "coverArt")]
    pub cover_art: Option<String>,
}

impl SubsonicAlbum {
    pub fn display_name(&self) -> String {
        if let Some(ref n) = self.name {
            if !n.is_empty() {
                return n.clone();
            }
        }
        if let Some(ref t) = self.title {
            if !t.is_empty() {
                return t.clone();
            }
        }
        "Untitled Album".to_string()
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct SubsonicArtist {
    pub id: String,
    pub name: String,
    #[serde(rename = "albumCount")]
    pub album_count: Option<u32>,
}

// --- Response Wrappers ---

#[derive(Debug, Deserialize)]
pub struct PlaylistsData {
    pub playlists: Option<PlaylistsContainer>,
}

#[derive(Debug, Deserialize)]
pub struct PlaylistsContainer {
    #[serde(default)]
    pub playlist: Vec<SubsonicPlaylist>,
}

#[derive(Debug, Deserialize)]
pub struct SongDetailData {
    pub song: Option<SubsonicSong>,
}

#[derive(Debug, Deserialize)]
pub struct PlaylistDetailData {
    pub playlist: Option<PlaylistDetailContainer>,
}

#[derive(Debug, Deserialize)]
pub struct PlaylistDetailContainer {
    #[serde(default)]
    pub entry: Vec<SubsonicSong>,
}

#[derive(Debug, Deserialize)]
pub struct Starred2Data {
    pub starred2: Option<Starred2Container>,
}

#[derive(Debug, Deserialize)]
pub struct Starred2Container {
    #[serde(default)]
    pub song: Vec<SubsonicSong>,
    #[serde(default)]
    pub album: Vec<SubsonicAlbum>,
    #[serde(default)]
    pub artist: Vec<SubsonicArtist>,
}

#[derive(Debug, Deserialize)]
pub struct AlbumListData {
    #[serde(rename = "albumList2")]
    pub album_list2: Option<AlbumList2Container>,
    #[serde(rename = "albumList")]
    pub album_list: Option<AlbumList2Container>,
}

#[derive(Debug, Deserialize)]
pub struct AlbumList2Container {
    #[serde(default)]
    pub album: Vec<SubsonicAlbum>,
}

#[derive(Debug, Deserialize)]
pub struct AlbumDetailData {
    pub album: Option<AlbumDetailContainer>,
}

#[derive(Debug, Deserialize)]
pub struct AlbumDetailContainer {
    #[serde(default)]
    pub song: Vec<SubsonicSong>,
    #[serde(default)]
    pub entry: Vec<SubsonicSong>,
}

#[derive(Debug, Deserialize)]
pub struct DirectoryData {
    pub directory: Option<DirectoryContainer>,
}

#[derive(Debug, Deserialize)]
pub struct DirectoryContainer {
    #[serde(default)]
    pub child: Vec<SubsonicSong>,
}

#[derive(Debug, Deserialize)]
pub struct ArtistDetailData {
    pub artist: Option<ArtistDetailContainer>,
}

#[derive(Debug, Deserialize)]
pub struct ArtistDetailContainer {
    #[serde(default)]
    pub album: Vec<SubsonicAlbum>,
}

#[derive(Debug, Deserialize)]
pub struct ArtistsData {
    pub artists: Option<ArtistsContainer>,
}

#[derive(Debug, Deserialize)]
pub struct ArtistsContainer {
    #[serde(default)]
    pub index: Vec<ArtistIndexItem>,
}

#[derive(Debug, Deserialize)]
pub struct ArtistIndexItem {
    pub name: String,
    #[serde(default)]
    pub artist: Vec<SubsonicArtist>,
}

#[derive(Debug, Deserialize)]
pub struct Search3Data {
    #[serde(rename = "searchResult3")]
    pub search_result3: Option<SearchResult3Container>,
}

#[derive(Debug, Deserialize)]
pub struct SearchResult3Container {
    #[serde(default)]
    pub song: Vec<SubsonicSong>,
    #[serde(default)]
    pub album: Vec<SubsonicAlbum>,
    #[serde(default)]
    pub artist: Vec<SubsonicArtist>,
}

impl SubsonicClient {
    pub fn new(config: NavidromeConfig) -> Self {
        Self {
            server_url: config.server_url,
            username: config.username,
            password: config.password,
        }
    }

    /// Construct a full URL for an OpenSubsonic endpoint with auth query parameters.
    pub fn build_url(&self, endpoint: &str, extra_params: &[(&str, &str)]) -> String {
        let clean_endpoint = endpoint.trim_start_matches('/');
        let base = format!("{}/rest/{}", self.server_url, clean_endpoint);

        let mut params = vec![
            ("u", self.username.as_str()),
            ("p", self.password.as_str()),
            ("v", SUBSONIC_VERSION),
            ("c", CLIENT_NAME),
            ("f", "json"),
        ];
        params.extend_from_slice(extra_params);

        let query = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, urlencode(v)))
            .collect::<Vec<_>>()
            .join("&");

        format!("{}?{}", base, query)
    }

    fn get_json<T: DeserializeOwned>(&self, endpoint: &str, params: &[(&str, &str)]) -> Result<T> {
        let url = self.build_url(endpoint, params);
        let res = ureq::get(&url)
            .config()
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .call()
            .map_err(|e| anyhow!("HTTP connection failed: {e}"))?;

        let status = res.status().as_u16();
        if status < 200 || status >= 300 {
            return Err(anyhow!("HTTP error status: {status}"));
        }

        let mut body = res.into_body();
        let wrapper: SubsonicResponseWrapper<T> = body
            .read_json()
            .context("Failed to parse JSON response from Navidrome server")?;

        if wrapper.response.status != "ok" {
            if let Some(err) = wrapper.response.error {
                return Err(anyhow!("Navidrome auth error ({}): {}", err.code, err.message));
            }
            return Err(anyhow!("Navidrome server returned status: {}", wrapper.response.status));
        }

        Ok(wrapper.response.data)
    }

    /// Test authentication and connection against `ping.view`.
    pub fn ping(&self) -> Result<()> {
        let _data: PingData = self.get_json("ping.view", &[])?;
        Ok(())
    }

    /// Fetch details for a single song by ID.
    pub fn get_song(&self, id: &str) -> Result<SubsonicSong> {
        let data: SongDetailData = self.get_json("getSong.view", &[("id", id)])?;
        data.song.ok_or_else(|| anyhow!("Song not found"))
    }

    /// Fetch all playlists.
    pub fn get_playlists(&self) -> Result<Vec<SubsonicPlaylist>> {
        let data: PlaylistsData = self.get_json("getPlaylists.view", &[])?;
        Ok(data.playlists.map(|p| p.playlist).unwrap_or_default())
    }

    /// Fetch tracks in a playlist.
    pub fn get_playlist_tracks(&self, playlist_id: &str) -> Result<Vec<SubsonicSong>> {
        let data: PlaylistDetailData = self.get_json("getPlaylist.view", &[("id", playlist_id)])?;
        Ok(data.playlist.map(|p| p.entry).unwrap_or_default())
    }

    /// Add a track to a playlist via Subsonic API updatePlaylist endpoint.
    pub fn add_to_playlist(&self, playlist_id: &str, song_id: &str) -> Result<()> {
        let _data: serde_json::Value = self.get_json(
            "updatePlaylist.view",
            &[("playlistId", playlist_id), ("songIdToAdd", song_id)],
        )?;
        Ok(())
    }

    /// Fetch starred (liked) songs, albums, and artists.
    pub fn get_starred(&self) -> Result<(Vec<SubsonicSong>, Vec<SubsonicAlbum>, Vec<SubsonicArtist>)> {
        let data: Starred2Data = self.get_json("getStarred2.view", &[])?;
        if let Some(starred) = data.starred2 {
            Ok((starred.song, starred.album, starred.artist))
        } else {
            Ok((vec![], vec![], vec![]))
        }
    }

    /// Fetch albums list.
    pub fn get_album_list(&self, list_type: &str, size: usize) -> Result<Vec<SubsonicAlbum>> {
        let sz_str = size.to_string();
        let types = [list_type, "newest", "frequent", "alphabeticalByName", "random"];
        for t in types {
            for endpoint in ["getAlbumList2.view", "getAlbumList.view"] {
                if let Ok(data) = self.get_json::<AlbumListData>(endpoint, &[("type", t), ("size", &sz_str)]) {
                    let albums = data.album_list2
                        .or(data.album_list)
                        .map(|c| c.album)
                        .unwrap_or_default();
                    if !albums.is_empty() {
                        return Ok(albums);
                    }
                }
            }
        }
        Ok(vec![])
    }

    /// Fetch tracks in an album.
    pub fn get_album_tracks(&self, album_id: &str) -> Result<Vec<SubsonicSong>> {
        if let Ok(data) = self.get_json::<AlbumDetailData>("getAlbum.view", &[("id", album_id)]) {
            if let Some(a) = data.album {
                if !a.song.is_empty() {
                    return Ok(a.song);
                }
                if !a.entry.is_empty() {
                    return Ok(a.entry);
                }
            }
        }
        if let Ok(data) = self.get_json::<DirectoryData>("getMusicDirectory.view", &[("id", album_id)]) {
            if let Some(d) = data.directory {
                if !d.child.is_empty() {
                    return Ok(d.child);
                }
            }
        }
        Ok(vec![])
    }

    /// Fetch albums for an artist.
    pub fn get_artist_albums(&self, artist_id: &str) -> Result<Vec<SubsonicAlbum>> {
        if let Ok(data) = self.get_json::<ArtistDetailData>("getArtist.view", &[("id", artist_id)]) {
            if let Some(a) = data.artist {
                if !a.album.is_empty() {
                    return Ok(a.album);
                }
            }
        }
        if let Ok(data) = self.get_json::<DirectoryData>("getMusicDirectory.view", &[("id", artist_id)]) {
            if let Some(d) = data.directory {
                let albums: Vec<SubsonicAlbum> = d
                    .child
                    .into_iter()
                    .map(|c| {
                        let name = c.display_title();
                        SubsonicAlbum {
                            id: c.id,
                            name: Some(name),
                            title: None,
                            artist: c.artist,
                            song_count: None,
                            cover_art: c.cover_art,
                        }
                    })
                    .collect();
                return Ok(albums);
            }
        }
        Ok(vec![])
    }

    /// Fetch all artists.
    pub fn get_artists(&self) -> Result<Vec<SubsonicArtist>> {
        let data: ArtistsData = self.get_json("getArtists.view", &[])?;
        let mut out = Vec::new();
        if let Some(artists) = data.artists {
            for idx in artists.index {
                out.extend(idx.artist);
            }
        }
        Ok(out)
    }

    /// Search across songs, albums, and artists.
    pub fn search(&self, query: &str) -> Result<(Vec<SubsonicSong>, Vec<SubsonicAlbum>, Vec<SubsonicArtist>)> {
        let data: Search3Data = self.get_json("search3.view", &[("query", query), ("songCount", "50"), ("albumCount", "20"), ("artistCount", "20")])?;
        if let Some(res) = data.search_result3 {
            Ok((res.song, res.album, res.artist))
        } else {
            Ok((vec![], vec![], vec![]))
        }
    }

    /// Download cover art bytes by cover art ID or song/album ID.
    pub fn get_cover_art(&self, id: &str) -> Result<Vec<u8>> {
        let url = self.build_url("getCoverArt.view", &[("id", id)]);
        let res = ureq::get(&url)
            .config()
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .call()
            .map_err(|e| anyhow!("Failed to fetch cover art: {e}"))?;

        let status = res.status().as_u16();
        if status < 200 || status >= 300 {
            return Err(anyhow!("Failed to fetch cover art: HTTP {status}"));
        }

        let mut bytes = Vec::new();
        res.into_body().as_reader().read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

fn urlencode(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_url() {
        let config = NavidromeConfig::new(
            "http://localhost:4533".to_string(),
            "admin".to_string(),
            "secret123".to_string(),
        );
        let client = SubsonicClient::new(config);
        let url = client.build_url("ping.view", &[("foo", "bar")]);
        assert!(url.starts_with("http://localhost:4533/rest/ping.view?"));
        assert!(url.contains("u=admin"));
        assert!(url.contains("p=secret123"));
        assert!(url.contains("v=1.16.1"));
        assert!(url.contains("c=myx"));
        assert!(url.contains("f=json"));
        assert!(url.contains("foo=bar"));
    }

    #[test]
    fn test_parse_ping_response() {
        let json_str = r#"{
            "subsonic-response": {
                "status": "ok",
                "version": "1.16.1"
            }
        }"#;
        let wrapper: SubsonicResponseWrapper<PingData> = serde_json::from_str(json_str).unwrap();
        assert_eq!(wrapper.response.status, "ok");
        assert_eq!(wrapper.response.version.as_deref(), Some("1.16.1"));
    }

    #[test]
    fn test_parse_playlists_response() {
        let json_str = r#"{
            "subsonic-response": {
                "status": "ok",
                "playlists": {
                    "playlist": [
                        { "id": "p1", "name": "Chill Vibes", "songCount": 15 }
                    ]
                }
            }
        }"#;
        let wrapper: SubsonicResponseWrapper<PlaylistsData> = serde_json::from_str(json_str).unwrap();
        let playlists = wrapper.response.data.playlists.unwrap().playlist;
        assert_eq!(playlists.len(), 1);
        assert_eq!(playlists[0].name, "Chill Vibes");
    }
}
