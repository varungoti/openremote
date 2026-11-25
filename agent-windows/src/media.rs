//! Real capture/encode pipeline. Managed builds supply a pinned capture executable
//! next to the host. Its SHA-256 is checked before execution; no shell is used.
//! No shell is used. H.264 access units (not individual NALs) are sent over WebRTC.
use crate::{
    config::{CaptureBackend, Config, VideoEncoder},
    input::Geometry,
};
use anyhow::{bail, ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::{path::Path, process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::AsyncReadExt,
    process::{Child, Command},
};
use tokio_util::sync::CancellationToken;
use webrtc::{
    media::Sample, track::track_local::track_local_static_sample::TrackLocalStaticSample,
};

const MAX_UNIT: usize = 4 * 1024 * 1024;

pub async fn verify_ffmpeg(path: &Path, expected: &str) -> Result<()> {
    ensure!(
        path.is_absolute() && path.is_file(),
        "FFmpeg must be an existing absolute file path"
    );
    let mut file = tokio::fs::File::open(path).await?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    let actual = format!("{:x}", hash.finalize());
    ensure!(
        actual.eq_ignore_ascii_case(expected),
        "FFmpeg hash mismatch; refusing to execute changed binary"
    );
    Ok(())
}

#[derive(Clone, Copy)]
pub enum CaptureSource {
    Desktop(Geometry, CaptureBackend),
    Synthetic,
}

pub fn ffmpeg_args(cfg: &Config, source: CaptureSource) -> Result<Vec<String>> {
    let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error"]
        .into_iter()
        .map(String::from)
        .collect();
    let fps = cfg.fps.to_string();
    let pixel_format = cfg.video_encoder.pixel_format();
    // h264_mf uses AVCodecContext's numeric profile/level options; libx264
    // exposes its own named profile and decimal-level options.
    let (profile, level) = match cfg.video_encoder {
        VideoEncoder::Libx264 => ("baseline", "3.1"),
        VideoEncoder::MediaFoundation => ("66", "31"),
    };
    let scale=format!("scale=w={}:h=720:force_original_aspect_ratio=decrease:force_divisible_by=2:flags=fast_bilinear,format={pixel_format}",cfg.max_width);
    match source {
        CaptureSource::Synthetic => {
            a.extend([
                "-re".into(),
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                format!("testsrc2=size=640x360:rate={}", cfg.fps),
                "-vf".into(),
                scale,
            ]);
        }
        CaptureSource::Desktop(g, CaptureBackend::Gdi) => {
            a.extend([
                "-f".into(),
                "gdigrab".into(),
                "-framerate".into(),
                fps.clone(),
                "-draw_mouse".into(),
                "1".into(),
                "-offset_x".into(),
                g.left.to_string(),
                "-offset_y".into(),
                g.top.to_string(),
                "-video_size".into(),
                format!("{}x{}", g.width, g.height),
                "-i".into(),
                "desktop".into(),
                "-vf".into(),
                scale,
            ]);
        }
        CaptureSource::Desktop(g, CaptureBackend::Dxgi) => {
            ensure!(g.monitors==1,"DXGI mode currently requires exactly one monitor; use GDI to select a display on multi-monitor PCs");
            a.extend([
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                format!("ddagrab=output_idx=0:framerate={}:draw_mouse=1", cfg.fps),
                "-vf".into(),
                format!("hwdownload,format=bgra,{scale}"),
            ]);
        }
    }
    // Baseline, no B frames, one-second recovery IDR. Codec-specific framing
    // below repeats parameter sets and guarantees an AUD for every access unit.
    a.extend([
        "-an".into(),
        "-c:v".into(),
        cfg.video_encoder.codec_name().into(),
        "-profile:v".into(),
        profile.into(),
        "-level:v".into(),
        level.into(),
        "-pix_fmt".into(),
        pixel_format.into(),
        "-bf".into(),
        "0".into(),
        "-b:v".into(),
        cfg.bitrate_bps.to_string(),
        "-maxrate".into(),
        cfg.bitrate_bps.to_string(),
        "-bufsize".into(),
        (cfg.bitrate_bps / 2).to_string(),
        "-g".into(),
        fps.clone(),
    ]);
    match cfg.video_encoder {
        VideoEncoder::Libx264 => a.extend([
            "-preset".into(),
            "ultrafast".into(),
            "-tune".into(),
            "zerolatency".into(),
            "-keyint_min".into(),
            fps,
            "-x264-params".into(),
            "repeat-headers=1:aud=1:scenecut=0".into(),
        ]),
        VideoEncoder::MediaFoundation => a.extend([
            "-flags".into(),
            "+low_delay".into(),
            "-rate_control".into(),
            "cbr".into(),
            "-scenario".into(),
            "display_remoting".into(),
            "-hw_encoding".into(),
            "0".into(),
            "-bsf:v".into(),
            "h264_mp4toannexb,dump_extra=freq=keyframe,h264_metadata=aud=insert".into(),
        ]),
    }
    a.extend(["-f".into(), "h264".into(), "pipe:1".into()]);
    Ok(a)
}

pub async fn stream(
    cfg: &Config,
    source: CaptureSource,
    track: Arc<TrackLocalStaticSample>,
    stop: CancellationToken,
    state: tokio::sync::watch::Receiver<
        webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState,
    >,
) -> Result<()> {
    stream_observed(cfg, source, track, stop, state, None).await
}

pub async fn stream_observed(
    cfg: &Config,
    source: CaptureSource,
    track: Arc<TrackLocalStaticSample>,
    stop: CancellationToken,
    state: tokio::sync::watch::Receiver<
        webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState,
    >,
    frames: Option<tokio::sync::watch::Sender<u64>>,
) -> Result<()> {
    verify_ffmpeg(&cfg.ffmpeg_path, &cfg.ffmpeg_sha256).await?;
    let args = ffmpeg_args(cfg, source)?;
    let mut command = Command::new(&cfg.ffmpeg_path);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .env_remove("OPENREMOTE_ENROLLMENT_TOKEN");
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let mut child = command.spawn().context("start FFmpeg")?;
    let Some(mut stdout) = child.stdout.take() else {
        reap_encoder(&mut child).await?;
        bail!("FFmpeg stdout unavailable");
    };
    let Some(mut stderr) = child.stderr.take() else {
        reap_encoder(&mut child).await?;
        bail!("FFmpeg stderr unavailable");
    };
    // Drain stderr continuously to avoid a deadlocked encoder, but retain only a bounded tail.
    let mut errors = tokio::spawn(async move {
        let mut tail = Vec::<u8>::new();
        let mut b = [0u8; 1024];
        while let Ok(n) = stderr.read(&mut b).await {
            if n == 0 {
                break;
            }
            tail.extend_from_slice(&b[..n]);
            if tail.len() > 8192 {
                tail.drain(..tail.len() - 8192);
            }
        }
        String::from_utf8_lossy(&tail).into_owned()
    });
    let mut parser = AccessUnits::default();
    let mut buf = [0u8; 65536];
    let result:Result<()>=async {
        loop {
            let n=tokio::select! {biased;_ = stop.cancelled()=>return Ok(()),n=tokio::time::timeout(Duration::from_secs(8),stdout.read(&mut buf))=>n.context("encoder stalled for 8 seconds")??};
            if n==0 {bail!("encoder exited before session ended");}
            for frame in parser.push(&buf[..n])? {
                // Keep draining the encoder while ICE recovers, dropping stale frames rather
                // than filling a pipe/queue or replaying old screen contents after reconnection.
                if *state.borrow() != webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState::Connected {continue;}
                let sample=Sample{data:frame.into(),duration:Duration::from_secs_f64(1.0/cfg.fps as f64),..Default::default()};
                let write = tokio::select! {biased;_ = stop.cancelled()=>return Ok(()),r=tokio::time::timeout(Duration::from_secs(3),track.write_sample(&sample))=>r};
                if *state.borrow() == webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState::Connected {
                    write.context("video sender stalled")??;
                    if let Some(frames) = &frames { frames.send_modify(|value| *value = value.saturating_add(1)); }
                }
            }
        }
    }.await;
    let reaped = reap_encoder(&mut child).await;
    let diagnostic = match tokio::time::timeout(Duration::from_secs(1), &mut errors).await {
        Ok(Ok(tail)) => tail,
        Ok(Err(_)) => String::new(),
        Err(_) => {
            errors.abort();
            let _ = errors.await;
            String::new()
        }
    };
    reaped?;
    result.with_context(|| {
        format!(
            "capture stopped: {}",
            diagnostic.chars().take(2048).collect::<String>()
        )
    })
}

/// A confirmed wait result is required even when kill reports the child already
/// exited. Requesting termination or dropping a kill-on-drop handle is not proof
/// that another capture session can start safely.
async fn reap_encoder(child: &mut Child) -> Result<()> {
    let _ = child.start_kill();
    match tokio::time::timeout(Duration::from_secs(2), child.wait()).await {
        Ok(Ok(_)) => Ok(()),
        _ => Err(crate::UnverifiedTeardown.into()),
    }
}

/// Streaming Annex-B parser. AUD marks an access-unit boundary; start codes may be split at
/// arbitrary read offsets. A bounded unit prevents malformed output consuming unlimited RAM.
#[derive(Default)]
pub struct AccessUnits {
    pending: Vec<u8>,
    unit: Vec<u8>,
    has_vcl: bool,
}
impl AccessUnits {
    pub fn push(&mut self, data: &[u8]) -> Result<Vec<Vec<u8>>> {
        self.pending.extend_from_slice(data);
        ensure!(self.pending.len() <= MAX_UNIT, "oversized H.264 NAL");
        let mut out = Vec::new();
        loop {
            let Some((start, prefix)) = find_start(&self.pending, 0) else {
                ensure!(self.pending.len() < 8, "missing Annex-B prefix");
                break;
            };
            ensure!(start == 0, "garbage before Annex-B prefix");
            let Some((next, _)) = find_start(&self.pending, prefix) else {
                break;
            };
            let nal: Vec<u8> = self.pending.drain(..next).collect();
            self.consume(&nal, prefix, &mut out)?;
        }
        Ok(out)
    }
    fn consume(&mut self, nal: &[u8], prefix: usize, out: &mut Vec<Vec<u8>>) -> Result<()> {
        ensure!(nal.len() > prefix, "empty H.264 NAL");
        let kind = nal[prefix] & 31;
        if kind == 9 && self.has_vcl {
            out.push(std::mem::take(&mut self.unit));
            self.has_vcl = false;
        }
        self.unit.extend_from_slice(nal);
        self.has_vcl |= matches!(kind, 1..=5);
        ensure!(self.unit.len() <= MAX_UNIT, "oversized H.264 access unit");
        Ok(())
    }
    pub fn finish(&mut self) -> Result<Vec<Vec<u8>>> {
        let mut out = Vec::new();
        let nal = std::mem::take(&mut self.pending);
        if !nal.is_empty() {
            let (_, prefix) = find_start(&nal, 0).context("invalid final NAL")?;
            self.consume(&nal, prefix, &mut out)?;
        }
        if self.has_vcl {
            out.push(std::mem::take(&mut self.unit));
            self.has_vcl = false;
        }
        Ok(out)
    }
}
fn find_start(data: &[u8], from: usize) -> Option<(usize, usize)> {
    for i in from..data.len().saturating_sub(2) {
        if data[i..].starts_with(&[0, 0, 0, 1]) {
            return Some((i, 4));
        }
        if data[i..].starts_with(&[0, 0, 1]) {
            return Some((i, 3));
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn access_units_survive_every_chunk_boundary() {
        let bytes = [
            0, 0, 0, 1, 9, 0xf0, 0, 0, 1, 0x67, 0x42, 0x01, 0, 0, 1, 0x68, 0x42, 0, 0, 1, 0x65,
            0x80, 1, 2, 0, 0, 1, 9, 0xf0, 0, 0, 1, 0x41, 0x80, 3, 4,
        ];
        for size in 1..=bytes.len() {
            let mut p = AccessUnits::default();
            let mut frames = Vec::new();
            for c in bytes.chunks(size) {
                frames.extend(p.push(c).unwrap());
            }
            frames.extend(p.finish().unwrap());
            assert_eq!(frames.len(), 2, "chunk {size}");
            assert_eq!(frames.concat(), bytes);
        }
    }
    #[test]
    fn bounds_reject_oversized_output() {
        let mut p = AccessUnits::default();
        let mut bytes = vec![0, 0, 0, 1, 0x65];
        bytes.resize(MAX_UNIT + 1, 0xff);
        assert!(p.push(&bytes).is_err());
    }
    #[test]
    fn args_cannot_be_shell_interpolated() {
        let cfg = Config::default();
        let a = ffmpeg_args(&cfg, CaptureSource::Synthetic).unwrap();
        assert!(a.contains(&"pipe:1".to_string()));
        assert!(!a.contains(&"-y".to_string()));
        assert!(a.contains(&"repeat-headers=1:aud=1:scenecut=0".to_string()));
    }
    #[test]
    fn bundled_windows_encoder_preserves_access_unit_and_decoder_recovery_contract() {
        let cfg = Config {
            video_encoder: VideoEncoder::MediaFoundation,
            ..Config::default()
        };
        let args = ffmpeg_args(&cfg, CaptureSource::Synthetic).unwrap();
        let value = |name: &str| args[args.iter().position(|v| v == name).unwrap() + 1].as_str();
        assert_eq!(value("-c:v"), "h264_mf");
        assert_eq!(value("-bf"), "0");
        assert_eq!(value("-pix_fmt"), "nv12");
        assert_eq!(value("-profile:v"), "66");
        assert_eq!(value("-level:v"), "31");
        assert_eq!(value("-g"), cfg.fps.to_string());
        assert_eq!(value("-flags"), "+low_delay");
        let filters = value("-bsf:v");
        assert!(filters.contains("h264_mp4toannexb"));
        assert!(filters.contains("dump_extra=freq=keyframe"));
        assert!(filters.contains("h264_metadata=aud=insert"));
        assert!(!args
            .iter()
            .any(|v| v == "-x264-params" || v == "-preset" || v == "-tune"));
        assert_eq!(args.last().unwrap(), "pipe:1");
    }
    #[test]
    fn selected_portrait_gdi_capture_preserves_negative_physical_offsets() {
        let g = Geometry {
            left: -1080,
            top: -200,
            width: 1080,
            height: 1920,
            monitors: 2,
        };
        let args = ffmpeg_args(
            &Config::default(),
            CaptureSource::Desktop(g, CaptureBackend::Gdi),
        )
        .unwrap();
        let value = |name: &str| args[args.iter().position(|v| v == name).unwrap() + 1].as_str();
        assert_eq!(value("-offset_x"), "-1080");
        assert_eq!(value("-offset_y"), "-200");
        assert_eq!(value("-video_size"), "1080x1920");
        assert!(ffmpeg_args(
            &Config::default(),
            CaptureSource::Desktop(g, CaptureBackend::Dxgi)
        )
        .is_err());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn encoder_cleanup_reaps_an_actual_child_before_returning() {
        let mut child = Command::new("/bin/sleep")
            .arg("30")
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        assert!(child.id().is_some());
        reap_encoder(&mut child).await.unwrap();
        assert!(child.id().is_none());
        assert!(child.try_wait().unwrap().is_some());
    }
}
