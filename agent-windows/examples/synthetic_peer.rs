//! A test-only peer: synthetic bars are the only captured source, and input is logged instead
//! of injected. It uses the production H.264 assembler, RTP peer and DataChannel validation.
//! No HTTP listener, backend credentials, persisted identity or desktop access is involved.
use anyhow::{bail, Context, Result};
use openremote_agent::{
    api::IceConfig,
    config::Config,
    displays::DisplayCatalog,
    media::{self, CaptureSource},
    protocol::PermissionSet,
    session::{ControlHandler, HostPeer},
    session_tools::ToolSession,
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use webrtc::peer_connection::{
    peer_connection_state::RTCPeerConnectionState, sdp::session_description::RTCSessionDescription,
};

#[tokio::main]
async fn main() -> Result<()> {
    eprintln!("SYNTHETIC TEST PEER: no desktop capture or input injection");
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if !raw.len().is_multiple_of(2) {
        bail!("arguments must be --name value pairs");
    }
    let args: HashMap<String, String> = raw
        .chunks(2)
        .map(|p| (p[0].clone(), p[1].clone()))
        .collect();
    for key in args.keys() {
        if ![
            "--offer-file",
            "--answer-file",
            "--events-file",
            "--ffmpeg",
            "--seconds",
        ]
        .contains(&key.as_str())
        {
            bail!("unknown argument {key}");
        }
    }
    let get = |name: &str| -> Result<String> {
        args.get(name)
            .cloned()
            .with_context(|| format!("missing {name}"))
    };
    let offer_file = PathBuf::from(get("--offer-file")?);
    let answer_file = PathBuf::from(get("--answer-file")?);
    let events = Arc::new(Mutex::new(
        std::fs::OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(get("--events-file")?)?,
    ));
    let duration = args
        .get("--seconds")
        .map(|s| s.parse::<u64>())
        .transpose()?
        .unwrap_or(45)
        .clamp(5, 60);
    let ffmpeg_path = PathBuf::from(get("--ffmpeg")?);
    let expected = format!("{:x}", Sha256::digest(tokio::fs::read(&ffmpeg_path).await?));
    let cfg = Config {
        ffmpeg_path,
        ffmpeg_sha256: expected,
        max_width: 640,
        fps: 30,
        bitrate_bps: 800_000,
        ..Default::default()
    };
    let handler: ControlHandler = Arc::new(move |bytes| {
        let mut f = events
            .lock()
            .map_err(|_| anyhow::anyhow!("event log poisoned"))?;
        f.write_all(bytes)?;
        f.write_all(b"\n")?;
        f.flush()?;
        Ok(())
    });
    let stop = CancellationToken::new();
    let permissions = PermissionSet {
        view_screen: true,
        control_input: true,
        ..Default::default()
    };
    // The production control channel emits both capabilities and this checked
    // primary-display catalog before browser input is allowed. Tools remain
    // permission-denied, so no clipboard or file operation can run here.
    let tools = Arc::new(ToolSession::new(permissions, stop.clone()));
    let peer = HostPeer::create_with_tools(
        IceConfig {
            ice_servers: Vec::new(),
            ice_transport_policy: "all".into(),
        },
        handler,
        stop.clone(),
        permissions,
        tools,
        DisplayCatalog::synthetic_test_pattern()?,
    )
    .await?;
    let offer: RTCSessionDescription = serde_json::from_slice(&tokio::fs::read(offer_file).await?)?;
    let answer = peer.answer(offer).await?;
    // Rename only after a complete SDP is durable, so the harness never reads partial JSON.
    let tmp = answer_file.with_extension("tmp");
    tokio::fs::write(&tmp, serde_json::to_vec(&answer)?).await?;
    tokio::fs::rename(tmp, answer_file).await?;
    let result:Result<()>=async {
        let mut state=peer.state.clone();
        tokio::time::timeout(Duration::from_secs(20),async{
            loop{if *state.borrow()==RTCPeerConnectionState::Connected{break;}
                tokio::select!{_ = stop.cancelled()=>bail!("peer stopped before connection"),r=state.changed()=>r?}
            }Ok::<(),anyhow::Error>(())
        }).await.context("synthetic peer connection timeout")??;
        eprintln!("SYNTHETIC TEST: WebRTC connected; streaming 640x360 H.264");
        let video=media::stream(&cfg,CaptureSource::Synthetic,peer.video.clone(),stop.clone(),peer.state.clone());
        tokio::select!{_ = tokio::time::sleep(Duration::from_secs(duration))=>Ok(()),r=video=>r,_ = tokio::signal::ctrl_c()=>Ok(())}
    }.await;
    peer.close().await?;
    result
}
