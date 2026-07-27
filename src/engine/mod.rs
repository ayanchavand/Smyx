//! Audio streaming engine for Navidrome / OpenSubsonic using `rodio`.
//!
//! Handles streaming audio playback from OpenSubsonic `/rest/stream.view` endpoints,
//! controls volume/playback state, taps PCM audio into `VisBands` for the FFT visualizer,
//! and dispatches `EngineEvent` updates to the app event loop.

pub mod auth;

use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use rodio::{Decoder, OutputStreamBuilder, Sink};

use crate::audio::{FftSource, VisBands};
use crate::subsonic::SubsonicClient;

/// A normalized playback event surfaced to the UI event loop.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    TrackChanged {
        uri: String,
    },
    Playing {
        uri: String,
        position_ms: u32,
    },
    Paused {
        uri: String,
        position_ms: u32,
    },
    Stopped,
    EndOfTrack {
        uri: String,
    },
    PositionCorrection {
        uri: String,
        position_ms: u32,
    },
}

/// The running audio engine: holds stream and Sink for audio output,
/// and updates `bands` for real-time visualizer rendering.
pub struct Engine {
    _stream: rodio::OutputStream,
    sink: Arc<Mutex<Sink>>,
    pub bands: Arc<Mutex<VisBands>>,
    pub client: SubsonicClient,
    pub current_track_uri: Arc<Mutex<Option<String>>>,
    event_tx: flume::Sender<EngineEvent>,
}

impl Engine {
    pub fn new(client: SubsonicClient, event_tx: flume::Sender<EngineEvent>) -> Result<Self> {
        let stream = OutputStreamBuilder::open_default_stream()
            .context("failed to initialize default audio output stream")?;
        let sink = Sink::connect_new(stream.mixer());
        let bands = VisBands::shared();

        Ok(Self {
            _stream: stream,
            sink: Arc::new(Mutex::new(sink)),
            bands,
            client,
            current_track_uri: Arc::new(Mutex::new(None)),
            event_tx,
        })
    }

    /// Play a track by its Subsonic song ID.
    pub fn play_track_id(&self, track_id: &str) -> Result<()> {
        let stream_url = self.client.build_url("stream.view", &[("id", track_id)]);
        let uri = format!("subsonic:track:{}", track_id);
        self.play_url(&stream_url, &uri)
    }

    /// Download / stream audio from a URL, wrap it in FFT visualizer tap, and play.
    pub fn play_url(&self, stream_url: &str, track_uri: &str) -> Result<()> {
        let req_client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .context("build reqwest client")?;

        let resp = req_client
            .get(stream_url)
            .send()
            .context("send audio stream request")?;

        if !resp.status().is_success() {
            return Err(anyhow!("Stream request failed with HTTP {}", resp.status()));
        }

        let bytes = resp.bytes().context("download audio stream bytes")?;
        let cursor = Cursor::new(bytes);
        let decoder = Decoder::new(cursor).context("decode audio format")?;

        // Decoder implements rodio::Source; wrap in FftSource for visualizer
        let fft_source = FftSource::new(decoder, self.bands.clone());

        let sink = self.sink.lock().unwrap();
        sink.stop();
        sink.append(fft_source);
        sink.play();

        if let Ok(mut curr) = self.current_track_uri.lock() {
            *curr = Some(track_uri.to_string());
        }

        let _ = self.event_tx.send(EngineEvent::TrackChanged {
            uri: track_uri.to_string(),
        });
        let _ = self.event_tx.send(EngineEvent::Playing {
            uri: track_uri.to_string(),
            position_ms: 0,
        });

        Ok(())
    }

    pub fn pause(&self) {
        let sink = self.sink.lock().unwrap();
        sink.pause();
        if let Ok(curr) = self.current_track_uri.lock() {
            if let Some(ref uri) = *curr {
                let _ = self.event_tx.send(EngineEvent::Paused {
                    uri: uri.clone(),
                    position_ms: 0,
                });
            }
        }
    }

    pub fn resume(&self) {
        let sink = self.sink.lock().unwrap();
        sink.play();
        if let Ok(curr) = self.current_track_uri.lock() {
            if let Some(ref uri) = *curr {
                let _ = self.event_tx.send(EngineEvent::Playing {
                    uri: uri.clone(),
                    position_ms: 0,
                });
            }
        }
    }

    pub fn toggle_play(&self) {
        let sink = self.sink.lock().unwrap();
        if sink.is_paused() {
            sink.play();
        } else {
            sink.pause();
        }
    }

    pub fn stop(&self) {
        let sink = self.sink.lock().unwrap();
        sink.stop();
        if let Ok(mut g) = self.bands.lock() {
            g.values.fill(0.0);
            g.is_active = false;
        }
        let _ = self.event_tx.send(EngineEvent::Stopped);
    }

    pub fn set_volume(&self, volume: f32) {
        let sink = self.sink.lock().unwrap();
        sink.set_volume(volume.clamp(0.0, 1.0));
    }

    pub fn is_playing(&self) -> bool {
        let sink = self.sink.lock().unwrap();
        !sink.is_paused() && !sink.empty()
    }
}
