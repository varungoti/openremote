//! Publisher-supplied capture runtime. Users never choose a path or checksum.
use crate::config::{Config, VideoEncoder};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use std::path::Path;

pub const PROFILE: &str = include_str!("../../config/encoder-profile.json");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EncoderProfile {
    schema_version: u32,
    file_name: String,
    version: String,
    encoder: String,
    sha256: String,
    size_bytes: u64,
}

fn apply(cfg: &mut Config, executable: &Path, raw: &str) -> Result<()> {
    ensure!(
        raw.len() <= 4096,
        "The application runtime profile is invalid."
    );
    let profile: EncoderProfile =
        serde_json::from_str(raw).context("The application runtime profile is invalid.")?;
    ensure!(
        profile.schema_version == 1
            && profile.file_name == "ffmpeg.exe"
            && profile.version == "9.0.2"
            && profile.encoder == "h264_mf"
            && profile.sha256.len() == 64
            && profile.sha256.bytes().all(|v| v.is_ascii_hexdigit())
            && (256..=32 * 1024 * 1024).contains(&profile.size_bytes),
        "The application runtime is not packaged correctly. Install a complete OpenRemote build."
    );
    ensure!(
        executable.is_absolute(),
        "The application location is invalid."
    );
    let directory = executable
        .parent()
        .context("The application location is invalid.")?;
    let path = directory.join(profile.file_name);
    let metadata = std::fs::metadata(&path)
        .context("The application runtime is missing. Reinstall OpenRemote.")?;
    ensure!(
        metadata.is_file() && metadata.len() == profile.size_bytes,
        "The application runtime has changed. Reinstall OpenRemote."
    );
    // SHA-256 verification happens again immediately before each process launch.
    // The profile carries no download URL, user-selected path, or secret.
    cfg.ffmpeg_path = path;
    cfg.ffmpeg_sha256 = profile.sha256.to_ascii_lowercase();
    cfg.video_encoder = VideoEncoder::MediaFoundation;
    Ok(())
}

pub fn configure(cfg: &mut Config) -> Result<()> {
    let executable = std::env::current_exe().context("The application location is unavailable.")?;
    apply(cfg, &executable, PROFILE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn profile(bytes: &[u8]) -> String {
        serde_json::json!({"schemaVersion":1,"fileName":"ffmpeg.exe","version":"9.0.2",
            "encoder":"h264_mf","sha256":format!("{:x}",Sha256::digest(bytes)),"sizeBytes":bytes.len()}).to_string()
    }

    #[test]
    fn packaged_runtime_ignores_legacy_user_path_and_preserves_permissions() {
        let dir =
            std::env::temp_dir().join(format!("openremote-runtime-{}", rand::random::<u64>()));
        std::fs::create_dir(&dir).unwrap();
        let bytes = [7u8; 256];
        std::fs::write(dir.join("ffmpeg.exe"), bytes).unwrap();
        let mut cfg = Config {
            allow_input: false,
            allow_unattended: true,
            trusted_controller_keys: vec!["owner-key".into()],
            ..Config::default()
        };
        apply(
            &mut cfg,
            &dir.join("openremote-agent.exe"),
            &profile(&bytes),
        )
        .unwrap();
        assert_eq!(cfg.ffmpeg_path, dir.join("ffmpeg.exe"));
        assert_eq!(cfg.video_encoder, VideoEncoder::MediaFoundation);
        assert!(!cfg.allow_input && cfg.allow_unattended);
        assert_eq!(cfg.trusted_controller_keys, ["owner-key"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn runtime_profile_rejects_missing_file_traversal_unknown_fields_and_bad_custody() {
        let bytes = [8u8; 256];
        let good = profile(&bytes);
        let mut cfg = Config::default();
        let dir =
            std::env::temp_dir().join(format!("profile-validation-{}", rand::random::<u64>()));
        let exe = dir.join("host.exe");
        assert!(apply(&mut cfg, &exe, &good).is_err());
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("ffmpeg.exe"), bytes).unwrap();
        apply(&mut cfg, &exe, &good).unwrap();
        for bad in [
            good.replace("ffmpeg.exe", "../ffmpeg.exe"),
            good.replace("h264_mf", "libx264"),
            good.replace("9.0.2", "0.0.0"),
            good.replace("\"schemaVersion\":1", "\"schemaVersion\":2"),
            good.replace("\"sizeBytes\":256", "\"sizeBytes\":0"),
            good.replace("{", "{\"url\":\"https://untrusted.invalid\","),
            good.replace("{", "{\"schemaVersion\":1,"),
        ] {
            assert!(apply(&mut cfg, &exe, &bad).is_err());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn same_size_encoder_replacement_is_rejected_before_execution() {
        let dir = std::env::temp_dir().join(format!("runtime-integrity-{}", rand::random::<u64>()));
        std::fs::create_dir(&dir).unwrap();
        let bytes = [9u8; 256];
        let path = dir.join("ffmpeg.exe");
        std::fs::write(&path, bytes).unwrap();
        let mut cfg = Config::default();
        apply(&mut cfg, &dir.join("host.exe"), &profile(&bytes)).unwrap();
        crate::media::verify_ffmpeg(&cfg.ffmpeg_path, &cfg.ffmpeg_sha256)
            .await
            .unwrap();
        std::fs::write(path, [10u8; 256]).unwrap();
        assert!(
            crate::media::verify_ffmpeg(&cfg.ffmpeg_path, &cfg.ffmpeg_sha256)
                .await
                .is_err()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
