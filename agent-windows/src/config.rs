use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// All settings are owner-controlled. First-enrollment keys arrive through the
/// local setup window or the legacy process environment and stay in memory.
/// They are never copied into this file. Unknown settings fail closed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub api_base_url: String,
    pub device_name: String,
    pub allow_unattended: bool,
    pub trusted_controller_keys: Vec<String>,
    pub allow_input: bool,
    pub allow_clipboard: bool,
    pub allow_file_transfer: bool,
    pub heartbeat_seconds: u64,
    pub ffmpeg_path: PathBuf,
    pub ffmpeg_sha256: String,
    pub video_encoder: VideoEncoder,
    pub capture_backend: CaptureBackend,
    pub fps: u32,
    pub max_width: u32,
    pub bitrate_bps: u32,
    pub allow_loopback_http: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CaptureBackend {
    Dxgi,
    Gdi,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VideoEncoder {
    Libx264,
    MediaFoundation,
}

impl VideoEncoder {
    pub fn codec_name(self) -> &'static str {
        match self {
            Self::Libx264 => "libx264",
            Self::MediaFoundation => "h264_mf",
        }
    }

    pub fn pixel_format(self) -> &'static str {
        match self {
            Self::Libx264 => "yuv420p",
            Self::MediaFoundation => "nv12",
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_base_url: "https://api.example.com".into(),
            device_name: std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Windows PC".into()),
            allow_unattended: false,
            trusted_controller_keys: Vec::new(),
            allow_input: true,
            allow_clipboard: false,
            allow_file_transfer: false,
            heartbeat_seconds: 15,
            ffmpeg_path: PathBuf::from(r"C:\Program Files\OpenRemote\ffmpeg.exe"),
            ffmpeg_sha256: String::new(),
            video_encoder: VideoEncoder::Libx264,
            capture_backend: CaptureBackend::Gdi,
            fps: 30,
            max_width: 1280,
            bitrate_bps: 4_000_000,
            allow_loopback_http: false,
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        let url = reqwest::Url::parse(&self.api_base_url).context("invalid API URL")?;
        let loopback = matches!(
            url.host_str(),
            Some("127.0.0.1" | "localhost" | "[::1]" | "::1")
        );
        if url.scheme() != "https"
            && !(self.allow_loopback_http && url.scheme() == "http" && loopback)
        {
            bail!("HTTPS is required (only explicit loopback development HTTP is supported)");
        }
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
        {
            bail!("API URL must be an origin without credentials, path, query or fragment");
        }
        if self.device_name.is_empty()
            || self.device_name.len() > 80
            || self.device_name.trim() != self.device_name
            || self
                .device_name
                .chars()
                .any(|c| c <= '\u{001f}' || c == '\u{007f}')
        {
            bail!("invalid device name");
        }
        if !(5..=30).contains(&self.heartbeat_seconds)
            || !(10..=30).contains(&self.fps)
            || !(640..=1280).contains(&self.max_width)
            || !(250_000..=30_000_000).contains(&self.bitrate_bps)
        {
            bail!("capture or heartbeat settings outside supported bounds");
        }
        if self.ffmpeg_sha256.len() != 64
            || !self.ffmpeg_sha256.bytes().all(|c| c.is_ascii_hexdigit())
        {
            bail!("set ffmpeg_sha256 to the SHA-256 of your trusted FFmpeg executable");
        }
        if self.trusted_controller_keys.len() > 32 {
            bail!("too many trusted controllers");
        }
        Ok(())
    }

    pub fn is_trusted(&self, key: &str) -> bool {
        self.allow_unattended
            && !key.is_empty()
            && self.trusted_controller_keys.iter().any(|v| v == key)
    }
}

pub fn data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("LOCALAPPDATA required"))
            .join("OpenRemote")
    }
    #[cfg(not(windows))]
    {
        std::env::temp_dir().join("openremote-test-host")
    }
}
pub fn config_path() -> PathBuf {
    data_dir().join("config.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn valid() -> Config {
        Config {
            ffmpeg_sha256: "a".repeat(64),
            ..Default::default()
        }
    }
    #[test]
    fn unattended_requires_both_flag_and_nonempty_exact_key() {
        let mut c = valid();
        c.trusted_controller_keys.push("abc".into());
        assert!(!c.is_trusted("abc"));
        c.allow_unattended = true;
        assert!(c.is_trusted("abc"));
        assert!(!c.is_trusted(""));
        assert!(!c.is_trusted("abc "));
    }
    #[test]
    fn transport_requires_tls_except_explicit_loopback() {
        let mut c = valid();
        assert!(c.validate().is_ok());
        c.api_base_url = "http://example.com".into();
        c.allow_loopback_http = true;
        assert!(c.validate().is_err());
        c.api_base_url = "http://127.0.0.1:8080".into();
        assert!(c.validate().is_ok());
        c.allow_loopback_http = false;
        assert!(c.validate().is_err());
        c.api_base_url = "https://u:p@example.com/".into();
        assert!(c.validate().is_err());
    }
    #[test]
    fn tools_default_off_and_heartbeat_stays_below_server_lease() {
        let mut c = valid();
        assert!(!c.allow_clipboard && !c.allow_file_transfer);
        for seconds in [5, 15, 30] {
            c.heartbeat_seconds = seconds;
            assert!(c.validate().is_ok());
        }
        for seconds in [0, 4, 31, 45, 60] {
            c.heartbeat_seconds = seconds;
            assert!(c.validate().is_err());
        }
    }
    #[test]
    fn device_name_matches_enrollment_bounds() {
        let mut c = valid();
        for name in ["", " leading", "trailing ", "a\nb", "a\u{007f}b"] {
            c.device_name = name.into();
            assert!(c.validate().is_err());
        }
        c.device_name = "é".repeat(40);
        assert!(c.validate().is_ok());
        c.device_name.push('x');
        assert!(c.validate().is_err());
    }
}
