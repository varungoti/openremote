//! Exercises the real encoder -> streaming access-unit assembler -> decoder, without sockets.
//! Only generated test bars are captured. The output path must not already exist.
use anyhow::{ensure, Context, Result};
use openremote_agent::{
    config::Config,
    media::{self, AccessUnits, CaptureSource},
};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::PathBuf, process::Stdio};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 2,
        "usage: capture_smoke ABS_FFMPEG NEW_OUTPUT.h264"
    );
    let ffmpeg_path = PathBuf::from(&args[0]);
    let path = PathBuf::from(&args[1]);
    ensure!(path.is_absolute(), "output must be absolute");
    let expected = format!("{:x}", Sha256::digest(tokio::fs::read(&ffmpeg_path).await?));
    let cfg = Config {
        ffmpeg_path,
        ffmpeg_sha256: expected,
        max_width: 640,
        fps: 30,
        bitrate_bps: 800_000,
        ..Default::default()
    };
    media::verify_ffmpeg(&cfg.ffmpeg_path, &cfg.ffmpeg_sha256).await?;
    let mut command_args = media::ffmpeg_args(&cfg, CaptureSource::Synthetic)?;
    // Limit the fixture at the encoder: all other flags come from the production builder.
    command_args.splice(
        command_args.len() - 3..command_args.len() - 3,
        ["-frames:v".to_string(), "30".to_string()],
    );
    let mut encoder = Command::new(&cfg.ffmpeg_path)
        .args(command_args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdout = encoder.stdout.take().context("capture pipe unavailable")?;
    let mut output = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .await?;
    let mut parser = AccessUnits::default();
    let mut read = [0u8; 4093];
    let mut frames = 0;
    loop {
        let n = stdout.read(&mut read).await?;
        if n == 0 {
            break;
        }
        for unit in parser.push(&read[..n])? {
            output.write_all(&unit).await?;
            frames += 1;
        }
    }
    for unit in parser.finish()? {
        output.write_all(&unit).await?;
        frames += 1;
    }
    output.sync_all().await?;
    drop(output);
    ensure!(encoder.wait().await?.success(), "encoder failed");
    ensure!(
        frames == 30,
        "assembler lost access units: expected30 got{frames}"
    );
    let decoded = Command::new(&cfg.ffmpeg_path)
        .args(["-v", "error", "-i"])
        .arg(&path)
        .args(["-map", "0:v:0", "-f", "framemd5", "-"])
        .stdin(Stdio::null())
        .output()
        .await?;
    ensure!(
        decoded.status.success(),
        "decoder failed: {}",
        String::from_utf8_lossy(&decoded.stderr)
    );
    let manifest = String::from_utf8(decoded.stdout)?;
    let rows: Vec<&str> = manifest
        .lines()
        .filter(|s| !s.starts_with('#') && !s.trim().is_empty())
        .collect();
    let hashes: HashSet<&str> = rows.iter().filter_map(|s| s.rsplit(',').next()).collect();
    ensure!(rows.len() == 30, "decoder did not recover30frames");
    ensure!(hashes.len() > 1, "synthetic video is frozen");
    println!(
        "{}",
        serde_json::json!({"ok":true,"fixture":"synthetic_capture_encode_assemble_decode","encodedAccessUnits":frames,"decodedFrames":rows.len(),"distinctFrameHashes":hashes.len(),"ffmpegSha256":cfg.ffmpeg_sha256,"output":path})
    );
    Ok(())
}
