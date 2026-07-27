//! OpenSubsonic / Navidrome REST client module.
//!
//! Connects to Navidrome server endpoints using plain password authentication (`u`, `p`).

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::time::Duration;

use crate::config::NavidromeConfig;

pub const SUBSONIC_VERSION: &str = "1.16.1";
pub const CLIENT_NAME: &str = "myx";

#[derive(Debug, Clone)]
pub struct SubsonicClient {
    pub server_url: String,
    pub username: String,
    pub password: String,
    client: Client,
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

impl SubsonicClient {
    pub fn new(config: NavidromeConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            server_url: config.server_url,
            username: config.username,
            password: config.password,
            client,
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

    /// Test authentication and connection against `ping.view`.
    pub fn ping(&self) -> Result<()> {
        let url = self.build_url("ping.view", &[]);
        let res = self
            .client
            .get(&url)
            .send()
            .context("Failed to connect to Navidrome server")?;

        if !res.status().is_success() {
            return Err(anyhow!("HTTP status error: {}", res.status()));
        }

        let wrapper: SubsonicResponseWrapper<PingData> = res
            .json()
            .context("Failed to parse JSON response from Navidrome server")?;

        if wrapper.response.status != "ok" {
            if let Some(err) = wrapper.response.error {
                return Err(anyhow!("Navidrome auth error ({}): {}", err.code, err.message));
            }
            return Err(anyhow!("Navidrome server returned status: {}", wrapper.response.status));
        }

        Ok(())
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
}
