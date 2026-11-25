use crate::{
    api::{Api, IceConfig, SessionInfo},
    config::Config,
    connections::{ConnectionState, Monitor},
    displays::{DisplayCatalog, DisplaySelection},
    identity::fingerprint,
    input::InputGuard,
    media::{self, CaptureSource},
    pairing::{PairingDecision, PairingGate},
    protocol::{parse_control, PermissionSet},
    session_tools::ToolSession,
    ui,
};
use anyhow::{bail, ensure, Context, Result};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::{sync::watch, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors,
        media_engine::{MediaEngine, MIME_TYPE_H264},
        APIBuilder,
    },
    data_channel::{data_channel_message::DataChannelMessage, RTCDataChannel},
    ice_transport::{
        ice_candidate::RTCIceCandidateInit, ice_connection_state::RTCIceConnectionState,
        ice_server::RTCIceServer,
    },
    interceptor::registry::Registry,
    peer_connection::{
        configuration::RTCConfiguration, peer_connection_state::RTCPeerConnectionState,
        policy::ice_transport_policy::RTCIceTransportPolicy,
        sdp::session_description::RTCSessionDescription, RTCPeerConnection,
    },
    rtp_transceiver::rtp_codec::RTCRtpCodecCapability,
    track::track_local::{track_local_static_sample::TrackLocalStaticSample, TrackLocal},
};

pub type ControlHandler = Arc<dyn Fn(&[u8]) -> Result<()> + Send + Sync>;

/// Shared by the Windows application and the synthetic browser integration example.
pub struct HostPeer {
    pub pc: Arc<RTCPeerConnection>,
    pub video: Arc<TrackLocalStaticSample>,
    pub state: watch::Receiver<RTCPeerConnectionState>,
    pub stop: CancellationToken,
    rtcp: JoinHandle<()>,
    recovery: JoinHandle<()>,
    tools: Option<Arc<ToolSession>>,
    tools_maintenance: Option<JoinHandle<()>>,
}
impl HostPeer {
    pub async fn create(
        ice: IceConfig,
        on_control: ControlHandler,
        stop: CancellationToken,
        control_allowed: bool,
    ) -> Result<Self> {
        Self::create_configured(
            ice,
            on_control,
            stop,
            PermissionSet {
                view_screen: true,
                control_input: control_allowed,
                ..Default::default()
            },
            None,
            None,
        )
        .await
    }
    pub async fn create_with_tools(
        ice: IceConfig,
        on_control: ControlHandler,
        stop: CancellationToken,
        permissions: PermissionSet,
        tools: Arc<ToolSession>,
        catalog: DisplayCatalog,
    ) -> Result<Self> {
        Self::create_configured(
            ice,
            on_control,
            stop,
            permissions,
            Some(tools),
            Some(catalog),
        )
        .await
    }
    async fn create_configured(
        ice: IceConfig,
        on_control: ControlHandler,
        stop: CancellationToken,
        permissions: PermissionSet,
        tools: Option<Arc<ToolSession>>,
        catalog: Option<DisplayCatalog>,
    ) -> Result<Self> {
        let mut engine = MediaEngine::default();
        engine.register_default_codecs()?;
        let registry = register_default_interceptors(Registry::new(), &mut engine)?;
        let api = APIBuilder::new()
            .with_media_engine(engine)
            .with_interceptor_registry(registry)
            .build();
        let config = RTCConfiguration {
            ice_servers: ice
                .ice_servers
                .into_iter()
                .map(|s| RTCIceServer {
                    urls: s.urls,
                    username: s.username,
                    credential: s.credential,
                })
                .collect(),
            ice_transport_policy: if ice.ice_transport_policy == "relay" {
                RTCIceTransportPolicy::Relay
            } else {
                RTCIceTransportPolicy::All
            },
            ..Default::default()
        };
        let pc = Arc::new(api.new_peer_connection(config).await?);
        let video = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_string(),
                clock_rate: 90000,
                sdp_fmtp_line:
                    "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f".into(),
                ..Default::default()
            },
            "desktop".into(),
            "openremote".into(),
        ));
        let sender = pc
            .add_track(video.clone() as Arc<dyn TrackLocal + Send + Sync>)
            .await?;
        let rtcp_stop = stop.clone();
        let rtcp = tokio::spawn(async move {
            loop {
                tokio::select! {_ = rtcp_stop.cancelled()=>break,r=sender.read_rtcp()=>if r.is_err(){break;}}
            }
        });
        let (tx, state) = watch::channel(RTCPeerConnectionState::New);
        let state_stop = stop.clone();
        let disconnect_handler = on_control.clone();
        pc.on_peer_connection_state_change(Box::new(move |s| {
            tx.send_replace(s);
            let stop = state_stop.clone();
            let handler = disconnect_handler.clone();
            Box::pin(async move {
                if matches!(
                    s,
                    RTCPeerConnectionState::Disconnected | RTCPeerConnectionState::Failed
                ) {
                    let _ = handler(br#"{"v":1,"type":"release_all"}"#);
                }
                if s == RTCPeerConnectionState::Closed {
                    stop.cancel();
                }
            })
        }));
        let recovery_stop = stop.clone();
        let recovery_state = state.clone();
        let recovery = tokio::spawn(async move {
            let mut since: Option<Instant> = None;
            loop {
                tokio::select! {_ = recovery_stop.cancelled()=>break, _ = tokio::time::sleep(Duration::from_millis(250))=>{}}
                let current = *recovery_state.borrow();
                if current == RTCPeerConnectionState::Connected {
                    since = None;
                } else if matches!(
                    current,
                    RTCPeerConnectionState::Disconnected | RTCPeerConnectionState::Failed
                ) {
                    since.get_or_insert_with(Instant::now);
                }
                if since.is_some_and(|when| when.elapsed() > Duration::from_secs(20)) {
                    recovery_stop.cancel();
                    break;
                }
            }
        });
        let has_control = Arc::new(AtomicBool::new(false));
        let has_tools = Arc::new(AtomicBool::new(false));
        let channel_stop = stop.clone();
        let channel_tools = tools.clone();
        pc.on_data_channel(Box::new(move |ch: Arc<RTCDataChannel>| {
            let stop = channel_stop.clone();
            let handler = on_control.clone();
            let tools = channel_tools.clone();
            let catalog = catalog.clone();
            let reliable = ch.ordered()
                && ch.max_retransmits().is_none()
                && ch.max_packet_lifetime().is_none();
            let control =
                reliable && ch.label() == "control" && !has_control.swap(true, Ordering::SeqCst);
            let tool = reliable
                && ch.label() == "tools"
                && tools.is_some()
                && !has_tools.swap(true, Ordering::SeqCst);
            Box::pin(async move {
                if control {
                    configure_control(&ch, handler, stop, permissions, tools.is_some(), catalog);
                } else if tool {
                    configure_tools(&ch, tools.expect("tools channel enabled"));
                } else {
                    let _ = ch.close().await;
                }
            })
        }));
        let tools_maintenance = tools.as_ref().map(|tools| {
            let tools=tools.clone();let stop=stop.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {_=stop.cancelled()=>{tools.revoke();break;},_=tokio::time::sleep(Duration::from_secs(1))=>{}}
                    if tools.is_closed() {tools.maintenance();break;}
                    let worker=tools.clone();
                    if tokio::task::spawn_blocking(move||worker.maintenance()).await.is_err() {tools.revoke();break;}
                }
            })
        });
        Ok(Self {
            pc,
            video,
            state,
            stop,
            rtcp,
            recovery,
            tools,
            tools_maintenance,
        })
    }
    pub async fn answer(&self, offer: RTCSessionDescription) -> Result<RTCSessionDescription> {
        ensure!(offer.sdp.len() <= 256 * 1024, "SDP exceeds limit");
        ensure!(
            offer.sdp_type == webrtc::peer_connection::sdp::sdp_type::RTCSdpType::Offer,
            "only a controller offer is accepted"
        );
        self.pc.set_remote_description(offer).await?;
        let answer = self.pc.create_answer(None).await?;
        let mut gathered = self.pc.gathering_complete_promise().await;
        self.pc.set_local_description(answer).await?;
        tokio::select! {_ = self.stop.cancelled()=>bail!("session stopped"),r=tokio::time::timeout(Duration::from_secs(20),gathered.recv())=>{r.context("ICE gathering timeout")?;}}
        self.pc
            .local_description()
            .await
            .context("missing local SDP")
    }
    pub async fn close(&self) -> Result<()> {
        self.stop.cancel();
        self.rtcp.abort();
        self.recovery.abort();
        if let Some(tools) = &self.tools {
            tools.revoke();
        }
        if let Some(task) = &self.tools_maintenance {
            task.abort();
        }
        match tokio::time::timeout(Duration::from_secs(3), self.pc.close()).await {
            Ok(Ok(())) => Ok(()),
            _ => Err(crate::UnverifiedTeardown.into()),
        }
    }
}
impl Drop for HostPeer {
    fn drop(&mut self) {
        self.stop.cancel();
        self.rtcp.abort();
        self.recovery.abort();
        if let Some(tools) = &self.tools {
            tools.revoke();
        }
        if let Some(task) = &self.tools_maintenance {
            task.abort();
        }
    }
}

fn configure_control(
    ch: &Arc<RTCDataChannel>,
    handler: ControlHandler,
    stop: CancellationToken,
    permissions: PermissionSet,
    has_tools: bool,
    catalog: Option<DisplayCatalog>,
) {
    let close_stop = stop.clone();
    ch.on_close(Box::new(move || {
        let stop = close_stop.clone();
        Box::pin(async move {
            stop.cancel();
        })
    }));
    let status_ch = Arc::downgrade(ch);
    let status_stop = stop.clone();
    ch.on_open(Box::new(move||{
        let ch=status_ch.upgrade();let catalog=catalog.clone();let stop=status_stop.clone();Box::pin(async move {
            if let Some(ch)=ch {
                let capabilities=serde_json::json!({"v":1,"type":"capabilities","text":permissions.control_input,"clipboard":has_tools&&permissions.clipboard,"multiMonitor":catalog.is_some(),"fileTransfer":has_tools&&permissions.file_transfer,"audio":false,"power":false});
                let result = tokio::time::timeout(Duration::from_secs(3),async {
                    ch.send_text(capabilities.to_string()).await?;
                    if let Some(catalog)=catalog {ch.send_text(serde_json::to_string(&catalog)?).await?;}
                    Ok::<(),anyhow::Error>(())
                }).await;
                if !matches!(result,Ok(Ok(()))) {stop.cancel();}
            }
        })
    }));
    let rate = Arc::new(Mutex::new((Instant::now(), 0u32)));
    ch.on_message(Box::new(move |msg: DataChannelMessage| {
        let stop = stop.clone();
        let handler = handler.clone();
        let rate = rate.clone();
        Box::pin(async move {
            if stop.is_cancelled() {
                return;
            }
            let allowed = {
                let Ok(mut r) = rate.lock() else {
                    stop.cancel();
                    return;
                };
                if r.0.elapsed() >= Duration::from_secs(1) {
                    *r = (Instant::now(), 0);
                }
                r.1 += 1;
                r.1 <= 400
            };
            if !allowed
                || !msg.is_string
                || msg.data.len() > 16 * 1024
                || parse_control(&msg.data).is_err()
            {
                stop.cancel();
                return;
            }
            if handler(&msg.data).is_err() {
                stop.cancel();
            }
        })
    }));
}
fn configure_tools(ch: &Arc<RTCDataChannel>, tools: Arc<ToolSession>) {
    let closed_tools = tools.clone();
    ch.on_close(Box::new(move || {
        let tools = closed_tools.clone();
        Box::pin(async move {
            tools.revoke();
        })
    }));
    let weak_ch = Arc::downgrade(ch);
    ch.on_message(Box::new(move |msg: DataChannelMessage| {
        let tools = tools.clone();
        let ch = weak_ch.upgrade();
        Box::pin(async move {
            let Some(ch) = ch else {
                tools.revoke();
                return;
            };
            if tools.is_closed()
                || !msg.is_string
                || msg.data.len() > crate::session_tools::protocol::MAX_FRAME
            {
                tools.revoke();
                let _ = ch.close().await;
                return;
            }
            let worker = tools.clone();
            let response = tokio::time::timeout(
                Duration::from_secs(10),
                tokio::task::spawn_blocking(move || worker.process(&msg.data)),
            )
            .await;
            if let Ok(Ok(Ok(response))) = response {
                if !tools.is_closed()
                    && matches!(
                        tokio::time::timeout(Duration::from_secs(3), ch.send_text(response)).await,
                        Ok(Ok(_))
                    )
                {
                    return;
                }
            }
            // I/O deadlines or framing failures only disable this tools channel.
            // Win32 calls already executing finish on their blocking worker and
            // observe revocation before returning or retaining a partial file.
            tools.revoke();
            let _ = ch.close().await;
        })
    }));
}

pub async fn run(
    api: Api,
    cfg: Config,
    request: SessionInfo,
    global_stop: CancellationToken,
    pairing: PairingGate,
) -> Result<()> {
    run_inner(api, cfg, request, global_stop, pairing, None).await
}

/// The managed app keeps a persistent visible owner window and emergency stop.
/// Other callers retain the separate consent/stop-notice behavior through run.
pub async fn run_with_owner_window(
    api: Api,
    cfg: Config,
    request: SessionInfo,
    global_stop: CancellationToken,
    pairing: PairingGate,
    monitor: Monitor,
) -> Result<()> {
    run_inner(api, cfg, request, global_stop, pairing, Some(monitor)).await
}

async fn run_inner(
    api: Api,
    cfg: Config,
    request: SessionInfo,
    global_stop: CancellationToken,
    pairing: PairingGate,
    monitor: Option<Monitor>,
) -> Result<()> {
    let stop = global_stop.child_token();
    let _cancel_on_exit = crate::host_home::RevokeOnExit(stop.clone());
    if request.status != "pending" {
        bail!("request is no longer pending");
    }
    let pairing_authorized = match pairing.consume(&request) {
        PairingDecision::Ordinary => false,
        PairingDecision::Authorized => true,
        PairingDecision::Reject => {
            api.action(&request.id, "reject").await?;
            return Ok(());
        }
    };
    struct ReleaseGrant {
        gate: PairingGate,
        request: SessionInfo,
    }
    impl Drop for ReleaseGrant {
        fn drop(&mut self) {
            self.gate.release(&self.request);
        }
    }
    let _grant = ReleaseGrant {
        gate: pairing.clone(),
        request: request.clone(),
    };
    let permissions = request.permissions.supported_tools(
        cfg.allow_input,
        cfg.allow_clipboard,
        cfg.allow_file_transfer,
    );
    ensure!(permissions.view_screen, "view permission required");
    media::verify_ffmpeg(&cfg.ffmpeg_path, &cfg.ffmpeg_sha256).await?;
    let display = DisplaySelection::current(request.monitor_id.as_deref())?;
    let geometry = display.layout.capture_geometry(display.index)?;
    let source = CaptureSource::Desktop(geometry, cfg.capture_backend);
    // Reject unsupported capture modes before asking the owner to consent.
    media::ffmpeg_args(&cfg, source)?;
    if !pairing_authorized && !cfg.is_trusted(&request.controller_public_key) {
        let name = request.controller_name.clone();
        let fp = fingerprint(&request.controller_public_key);
        let consent_stop = stop.clone();
        let display_name = display.description();
        let allowed = tokio::task::spawn_blocking(move || {
            ui::approve(&name, &fp, permissions, &display_name, consent_stop)
        })
        .await??;
        if !allowed {
            api.action(&request.id, "reject").await?;
            return Ok(());
        }
    }
    // Approval prompts can remain open; re-check expiration/revocation after owner interaction.
    let current = api.session(&request.id).await?;
    ensure!(
        current.status == "pending",
        "request expired while awaiting approval"
    );
    ensure!(
        current.controller_public_key == request.controller_public_key,
        "controller identity changed"
    );
    ensure!(
        current.monitor_id == request.monitor_id,
        "selected monitor changed"
    );
    ensure!(
        current.pairing_id == request.pairing_id && current.mode == request.mode,
        "code authorization changed"
    );
    ensure!(
        current.permissions == request.permissions,
        "requested permissions changed"
    );
    display.verify_current()?;
    ensure!(pairing.authorized_now(&request), "activation was revoked");
    let _notice = if monitor.is_some() {
        None
    } else {
        Some(ui::Notice::show(
            &request.controller_name,
            &display.description(),
            stop.clone(),
        )?)
    };
    ensure!(!stop.is_cancelled(), "local session stopped");
    api.approve(&request.id, permissions).await?;
    let input = Arc::new(Mutex::new(InputGuard::for_display(display.clone())));
    let input_handler = input.clone();
    let input_stop = stop.clone();
    let input_grant = pairing.clone();
    let input_request = request.clone();
    let handler: ControlHandler = Arc::new(move |bytes| {
        ensure!(
            input_grant.authorized_now(&input_request),
            "activation was revoked"
        );
        let cmd = parse_control(bytes)?;
        let mut input = input_handler
            .lock()
            .map_err(|_| anyhow::anyhow!("input lock poisoned"))?;
        apply_control_locked(&mut input, &input_stop, &cmd, permissions)
    });
    let peer = Arc::new(
        HostPeer::create_with_tools(
            api.ice(&request.id).await?,
            handler,
            stop.clone(),
            permissions,
            Arc::new(ToolSession::new(permissions, stop.clone())),
            display.catalog.clone(),
        )
        .await?,
    );
    let mut result = serve(
        api.clone(),
        &cfg,
        &request.id,
        peer.clone(),
        display,
        input.clone(),
        monitor,
    )
    .await;
    stop.cancel();
    if let Ok(mut guard) = input.lock() {
        if guard.release_all().is_err() {
            result = Err(crate::UnverifiedTeardown.into());
        }
    } else {
        result = Err(crate::UnverifiedTeardown.into());
    }
    if let Err(error) = peer.close().await {
        result = Err(error);
    }
    result
}

async fn serve(
    api: Api,
    cfg: &Config,
    id: &str,
    peer: Arc<HostPeer>,
    display: DisplaySelection,
    input: Arc<Mutex<InputGuard>>,
    monitor: Option<Monitor>,
) -> Result<()> {
    let source = CaptureSource::Desktop(
        display.layout.capture_geometry(display.index)?,
        cfg.capture_backend,
    );
    let topology_stop = peer.stop.clone();
    let mut topology = tokio::spawn(async move {
        let mut timer = tokio::time::interval(Duration::from_millis(500));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! { biased; _=topology_stop.cancelled()=>return Ok::<(),anyhow::Error>(()),_=timer.tick()=>{} }
            if let Err(error) = display.verify_current() {
                // Release held inputs immediately; never map them onto the new
                // desktop or quietly resume capture from a replacement monitor.
                topology_stop.cancel();
                if let Ok(mut input) = input.lock() {
                    let _ = input.release_all();
                }
                return Err(error);
            }
        }
    });
    let signal_peer = peer.clone();
    let signal_api = api.clone();
    let sid = id.to_owned();
    let (answered_tx, answered_rx) = watch::channel(0u64);
    let mut signaling =
        tokio::spawn(
            async move { signal_loop(&signal_api, &sid, &signal_peer, answered_tx).await },
        );
    let report_peer = peer.clone();
    let report_api = api.clone();
    let report_id = id.to_owned();
    let (frames_tx, frames_rx) = watch::channel(0u64);
    let mut connection_status = tokio::spawn(async move {
        report_connections(
            report_api,
            report_id,
            report_peer,
            answered_rx,
            frames_rx,
            monitor,
        )
        .await
    });
    let status_api = api.clone();
    let status_id = id.to_owned();
    let status_stop = peer.stop.clone();
    let mut status = tokio::spawn(async move {
        let mut failures = 0;
        loop {
            tokio::select! {_ = status_stop.cancelled()=>return Ok::<(),anyhow::Error>(()),_ = tokio::time::sleep(Duration::from_secs(2))=>{}}
            match tokio::time::timeout(Duration::from_secs(5), status_api.session(&status_id)).await
            {
                Ok(Ok(s)) => {
                    failures = 0;
                    if !matches!(s.status.as_str(), "approved" | "connecting" | "active") {
                        status_stop.cancel();
                        return Ok(());
                    }
                }
                Ok(Err(_)) | Err(_) => {
                    failures += 1;
                    if failures >= 3 {
                        status_stop.cancel();
                        bail!("session authorization could not be refreshed");
                    }
                }
            }
        }
    });
    let stream_peer = peer.clone();
    let cfg = cfg.clone();
    let mut video = tokio::spawn(async move {
        let mut state = stream_peer.state.clone();
        tokio::time::timeout(Duration::from_secs(45),async {
            loop {if *state.borrow()==RTCPeerConnectionState::Connected {break;}
                tokio::select!{_ = stream_peer.stop.cancelled()=>bail!("connection cancelled"),r=state.changed()=>r?}
            }Ok::<(),anyhow::Error>(())
        }).await.context("remote peer did not connect in 45 seconds")??;
        media::stream_observed(
            &cfg,
            source,
            stream_peer.video.clone(),
            stream_peer.stop.clone(),
            stream_peer.state.clone(),
            Some(frames_tx),
        )
        .await
    });
    // Do not use ? in a select arm: a panicked child must still cancel and join
    // every other task, especially the task holding the native encoder process.
    let (completed, result) = tokio::select! {
        _ = peer.stop.cancelled()=>(None, Ok(())),
        r=&mut signaling=>joined_task(0,r),
        r=&mut connection_status=>joined_task(1,r),
        r=&mut status=>joined_task(2,r),
        r=&mut video=>joined_task(3,r),
        r=&mut topology=>joined_task(4,r),
    };
    peer.stop.cancel();
    finish_session_tasks(
        vec![signaling, connection_status, status, video, topology],
        3,
        completed,
        result,
    )
    .await
}

fn apply_control_locked(
    input: &mut InputGuard,
    stop: &CancellationToken,
    command: &crate::protocol::ControlMessage,
    permissions: PermissionSet,
) -> Result<()> {
    // The caller holds the same mutex used by final release_all. Cancellation
    // is checked after acquiring it, so a queued callback cannot inject after
    // teardown has already released the last tracked inputs.
    ensure!(!stop.is_cancelled(), "local access revoked");
    input.apply(command, permissions)
}

fn joined_task(
    index: usize,
    outcome: std::result::Result<Result<()>, tokio::task::JoinError>,
) -> (Option<usize>, Result<()>) {
    (
        Some(index),
        match outcome {
            Ok(result) => result,
            Err(_) => Err(crate::UnverifiedTeardown.into()),
        },
    )
}

async fn finish_session_tasks(
    tasks: Vec<JoinHandle<Result<()>>>,
    capture_index: usize,
    completed: Option<usize>,
    mut outcome: Result<()>,
) -> Result<()> {
    // Non-capture futures contain only bounded API polling/state observation and
    // can be cancelled immediately. Still await the actual abort completion.
    for (index, task) in tasks.iter().enumerate() {
        if completed != Some(index) && index != capture_index {
            task.abort();
        }
    }
    for (index, mut task) in tasks.into_iter().enumerate() {
        if completed == Some(index) {
            continue;
        }
        let capture = index == capture_index;
        let limit = if capture {
            Duration::from_secs(4)
        } else {
            Duration::from_secs(1)
        };
        let next = match tokio::time::timeout(limit, &mut task).await {
            Ok(Ok(result)) => result,
            Ok(Err(error)) if !capture && error.is_cancelled() => Ok(()),
            Ok(Err(_)) => Err(crate::UnverifiedTeardown.into()),
            Err(_) => {
                task.abort();
                let _ = tokio::time::timeout(Duration::from_secs(1), &mut task).await;
                // Aborting a capture future can only request kill-on-drop. It
                // cannot prove the OS child was reaped, so never release busy.
                Err(crate::UnverifiedTeardown.into())
            }
        };
        if let Err(error) = next {
            if error.is::<crate::UnverifiedTeardown>() || outcome.is_ok() {
                outcome = Err(error);
            }
        }
    }
    outcome
}
async fn signal_loop(
    api: &Api,
    id: &str,
    peer: &HostPeer,
    answered: watch::Sender<u64>,
) -> Result<()> {
    let mut seq = 0;
    let mut pending = Vec::<RTCIceCandidateInit>::new();
    let mut has_remote = false;
    let mut offer_count = 0;
    loop {
        let mut retries = 0;
        let signals = loop {
            let result =
                tokio::select! {_ = peer.stop.cancelled()=>return Ok(()),r=api.signals(id,seq)=>r};
            match result {
                Ok(signals) => break signals,
                Err(error) => {
                    if let Some(failure) = error.downcast_ref::<crate::api::ApiFailure>() {
                        if matches!(failure.status, 400 | 401 | 403 | 404 | 409 | 410) {
                            return Err(error);
                        }
                    }
                    retries += 1;
                    if retries > 3 {
                        return Err(error.context("signaling retry budget exhausted"));
                    }
                    tokio::select! {_ = peer.stop.cancelled()=>return Ok(()), _ = tokio::time::sleep(Duration::from_secs(1<<retries))=>{}}
                }
            }
        };
        for s in signals {
            if s.from != "controller" {
                bail!("invalid signaling sender");
            }
            match s.kind.as_str() {
                "offer" => {
                    offer_count += 1;
                    ensure!(offer_count <= 32, "excessive renegotiation");
                    let fresh = api.ice(id).await?;
                    let mut config = peer.pc.get_configuration().await;
                    config.ice_servers = fresh
                        .ice_servers
                        .into_iter()
                        .map(|server| RTCIceServer {
                            urls: server.urls,
                            username: server.username,
                            credential: server.credential,
                        })
                        .collect();
                    config.ice_transport_policy = if fresh.ice_transport_policy == "relay" {
                        RTCIceTransportPolicy::Relay
                    } else {
                        RTCIceTransportPolicy::All
                    };
                    peer.pc.set_configuration(config).await?;
                    let description: RTCSessionDescription = serde_json::from_value(s.payload)?;
                    // A new offer is not media-ready until its answer and a fresh
                    // sample are acknowledged; DTLS may remain Connected.
                    answered.send_replace(0);
                    let answer = peer.answer(description).await?;
                    has_remote = true;
                    for c in pending.drain(..) {
                        peer.pc.add_ice_candidate(c).await?;
                    }
                    api.send_signal(id, "answer", serde_json::to_value(answer)?)
                        .await?;
                    // Notify only after the answer is accepted by the server.
                    // A stale Connected callback must not mark an unanswered
                    // offer active, and a restart may keep DTLS Connected.
                    answered.send_replace(offer_count);
                }
                "ice" => {
                    let c: RTCIceCandidateInit = serde_json::from_value(s.payload)?;
                    if has_remote {
                        peer.pc.add_ice_candidate(c).await?;
                    } else {
                        ensure!(pending.len() < 128, "too many pre-offer ICE candidates");
                        pending.push(c);
                    }
                }
                "renegotiate" => {}
                _ => bail!("unsupported controller signal"),
            }
            seq = seq.max(s.seq);
        }
    }
}

async fn report_connections(
    api: Api,
    id: String,
    peer: Arc<HostPeer>,
    mut answered: watch::Receiver<u64>,
    mut frames: watch::Receiver<u64>,
    monitor: Option<Monitor>,
) -> Result<()> {
    struct ClearCountOnExit(Option<Monitor>);
    impl Drop for ClearCountOnExit {
        fn drop(&mut self) {
            if let Some(monitor) = &self.0 {
                monitor.set_state(ConnectionState::Reconnecting);
            }
        }
    }
    let _clear_count_on_exit = ClearCountOnExit(monitor.clone());
    let mut state = peer.state.clone();
    let mut reported = 0;
    let mut failures = 0;
    let mut was_connected = false;
    let mut required_frame = 1;
    let mut last_answered = 0;
    let mut timer = tokio::time::interval(Duration::from_millis(200));
    loop {
        tokio::select! {
            _=peer.stop.cancelled()=>return Ok(()),
            r=answered.changed()=>if r.is_err(){return Ok(());},
            r=state.changed()=>if r.is_err(){return Ok(());},
            r=frames.changed()=>if r.is_err(){return Ok(());},
            _=timer.tick()=>{},
        }
        let generation = *answered.borrow();
        let healthy = *state.borrow() == RTCPeerConnectionState::Connected
            && matches!(
                peer.pc.ice_connection_state(),
                RTCIceConnectionState::Connected | RTCIceConnectionState::Completed
            );
        if generation != last_answered {
            required_frame = frames.borrow().saturating_add(1);
            last_answered = generation;
        }
        if !healthy || generation == 0 || reported != generation {
            if let Some(monitor) = &monitor {
                monitor.set_state(if was_connected {
                    ConnectionState::Reconnecting
                } else {
                    ConnectionState::Connecting
                });
            }
            if !healthy || generation == 0 {
                reported = 0;
                required_frame = frames.borrow().saturating_add(1);
            }
        }
        if *frames.borrow() >= required_frame
            && should_report_connected(
                generation,
                reported,
                *state.borrow(),
                peer.pc.ice_connection_state(),
            )
        {
            let result = tokio::select! {
                _=peer.stop.cancelled()=>return Ok(()),
                r=tokio::time::timeout(Duration::from_secs(5),api.action(&id,"connected"))=>r,
            };
            if matches!(result, Ok(Ok(()))) {
                reported = generation;
                failures = 0;
                if !peer.stop.is_cancelled()
                    && acknowledged_media_ready(
                        *answered.borrow(),
                        generation,
                        *frames.borrow(),
                        required_frame,
                        *state.borrow(),
                        peer.pc.ice_connection_state(),
                    )
                {
                    if let Some(monitor) = &monitor {
                        monitor.set_state(ConnectionState::Connected);
                    }
                    was_connected = true;
                }
            } else {
                failures += 1;
                ensure!(failures < 3, "connected status could not be acknowledged");
            }
        }
    }
}

fn acknowledged_media_ready(
    answered: u64,
    acknowledged: u64,
    frames: u64,
    required_frame: u64,
    state: RTCPeerConnectionState,
    ice: RTCIceConnectionState,
) -> bool {
    answered != 0
        && answered == acknowledged
        && frames >= required_frame
        && should_report_connected(answered, 0, state, ice)
}

fn should_report_connected(
    answered: u64,
    reported: u64,
    state: RTCPeerConnectionState,
    ice: RTCIceConnectionState,
) -> bool {
    answered > reported
        && state == RTCPeerConnectionState::Connected
        && matches!(
            ice,
            RTCIceConnectionState::Connected | RTCIceConnectionState::Completed
        )
}

#[derive(Debug, PartialEq, Eq)]
pub enum RequestDecision {
    Ignore,
    Start,
    RejectBusy,
    EndOrphan,
}
/// Retained events are hints; current authenticated session state is authoritative.
pub fn request_decision(request: &SessionInfo, active_id: Option<&str>) -> RequestDecision {
    match request.status.as_str() {
        "pending" => match active_id {
            Some(id) if id == request.id => RequestDecision::Ignore,
            Some(_) => RequestDecision::RejectBusy,
            None => RequestDecision::Start,
        },
        "approved" | "connecting" | "active" if active_id.is_none() => RequestDecision::EndOrphan,
        _ => RequestDecision::Ignore,
    }
}

/// The caller must obtain this status from the authenticated coordinator for
/// the currently running session. An end acknowledgement permits cancellation,
/// but the next session must still wait for all normal local teardown to finish.
/// On timeout the task stays owned; a consumed panicked task is reported
/// separately from an active session or a still-running cleanup operation.
#[derive(Debug, PartialEq, Eq)]
pub enum TeardownWait {
    StillActive,
    Finished,
    TimedOut,
    Failed,
}

pub async fn finish_terminal_session(
    status: &str,
    stop: &CancellationToken,
    task: &mut JoinHandle<()>,
    limit: Duration,
) -> TeardownWait {
    if !matches!(status, "ended" | "rejected" | "expired") {
        return TeardownWait::StillActive;
    }
    stop.cancel();
    match tokio::time::timeout(limit, task).await {
        Ok(Ok(())) => TeardownWait::Finished,
        Ok(Err(_)) => TeardownWait::Failed,
        Err(_) => TeardownWait::TimedOut,
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[test]
    fn connected_count_requires_current_answer_media_and_service_acknowledgment() {
        use RTCIceConnectionState as Ice;
        use RTCPeerConnectionState as Peer;
        assert!(!acknowledged_media_ready(
            0,
            0,
            0,
            1,
            Peer::Connected,
            Ice::Connected
        ));
        assert!(!acknowledged_media_ready(
            1,
            1,
            0,
            1,
            Peer::Connected,
            Ice::Connected
        ));
        assert!(!acknowledged_media_ready(
            1,
            0,
            1,
            1,
            Peer::Connected,
            Ice::Connected
        ));
        assert!(!acknowledged_media_ready(
            2,
            1,
            2,
            2,
            Peer::Connected,
            Ice::Connected
        ));
        assert!(!acknowledged_media_ready(
            1,
            1,
            1,
            1,
            Peer::Disconnected,
            Ice::Disconnected
        ));
        assert!(!acknowledged_media_ready(
            1,
            1,
            1,
            2,
            Peer::Connected,
            Ice::Completed
        ));
        assert!(acknowledged_media_ready(
            2,
            2,
            2,
            2,
            Peer::Connected,
            Ice::Completed
        ));
    }

    #[test]
    fn restarts_are_reported_after_answer_and_healthy_ice_even_without_new_dtls_callback() {
        use RTCPeerConnectionState::Connected;
        assert!(!should_report_connected(
            0,
            0,
            Connected,
            RTCIceConnectionState::Connected
        ));
        assert!(should_report_connected(
            1,
            0,
            Connected,
            RTCIceConnectionState::Connected
        ));
        assert!(!should_report_connected(
            1,
            1,
            Connected,
            RTCIceConnectionState::Connected
        ));
        assert!(!should_report_connected(
            2,
            1,
            Connected,
            RTCIceConnectionState::Checking
        ));
        assert!(should_report_connected(
            2,
            1,
            Connected,
            RTCIceConnectionState::Completed
        ));
        assert!(!should_report_connected(
            2,
            1,
            RTCPeerConnectionState::Disconnected,
            RTCIceConnectionState::Connected
        ));
    }
    #[test]
    fn historical_sessions_do_not_claim_busy_or_reject_current_pending_request() {
        let mut request = SessionInfo {
            id: "current".into(),
            status: "ended".into(),
            permissions: Default::default(),
            controller_name: String::new(),
            controller_public_key: String::new(),
            monitor_id: None,
            pairing_id: None,
            mode: None,
            host_name: String::new(),
        };
        assert_eq!(request_decision(&request, None), RequestDecision::Ignore);
        request.status = "pending".into();
        assert_eq!(request_decision(&request, None), RequestDecision::Start);
        assert_eq!(
            request_decision(&request, Some("current")),
            RequestDecision::Ignore
        );
        assert_eq!(
            request_decision(&request, Some("different")),
            RequestDecision::RejectBusy
        );
        request.status = "active".into();
        assert_eq!(request_decision(&request, None), RequestDecision::EndOrphan);
        assert_eq!(
            request_decision(&request, Some("current")),
            RequestDecision::Ignore
        );
        assert_eq!(
            request_decision(&request, Some("different")),
            RequestDecision::Ignore
        );
    }
    #[tokio::test]
    async fn replacement_waits_for_terminal_session_cleanup_without_aborting_live_task() {
        let stop = CancellationToken::new();
        let observed = Arc::new(AtomicBool::new(false));
        let completed = observed.clone();
        let task_stop = stop.clone();
        let mut task = tokio::spawn(async move {
            task_stop.cancelled().await;
            completed.store(true, Ordering::SeqCst);
        });
        assert_eq!(
            finish_terminal_session("active", &stop, &mut task, Duration::from_secs(1)).await,
            TeardownWait::StillActive
        );
        assert!(!stop.is_cancelled());
        assert!(!observed.load(Ordering::SeqCst));
        assert_eq!(
            finish_terminal_session("ended", &stop, &mut task, Duration::from_secs(1)).await,
            TeardownWait::Finished
        );
        assert!(observed.load(Ordering::SeqCst));
    }
    #[tokio::test]
    async fn cleanup_timeout_keeps_the_old_task_and_rejects_replacement() {
        let stop = CancellationToken::new();
        let (done, wait) = tokio::sync::oneshot::channel::<()>();
        let mut task = tokio::spawn(async move {
            let _ = wait.await;
        });
        assert_eq!(
            finish_terminal_session("ended", &stop, &mut task, Duration::ZERO).await,
            TeardownWait::TimedOut
        );
        assert!(stop.is_cancelled());
        assert!(!task.is_finished());
        done.send(()).unwrap();
        assert_eq!(
            finish_terminal_session("ended", &stop, &mut task, Duration::from_secs(1)).await,
            TeardownWait::Finished
        );
    }
    #[tokio::test]
    async fn completed_task_with_stale_active_status_is_not_a_cleanup_failure() {
        let stop = CancellationToken::new();
        let mut task = tokio::spawn(async {});
        tokio::task::yield_now().await;
        assert_eq!(
            finish_terminal_session("active", &stop, &mut task, Duration::from_secs(1)).await,
            TeardownWait::StillActive
        );
        assert_eq!(
            finish_terminal_session("ended", &stop, &mut task, Duration::from_secs(1)).await,
            TeardownWait::Finished
        );
        let mut panicked = tokio::spawn(async {
            panic!("synthetic session panic");
        });
        assert_eq!(
            finish_terminal_session("ended", &stop, &mut panicked, Duration::from_secs(1)).await,
            TeardownWait::Failed
        );
    }
    #[tokio::test]
    async fn panicked_child_still_awaits_capture_cleanup_and_latches_restart_required() {
        let failed = tokio::spawn(async {
            panic!("synthetic signaling panic");
            #[allow(unreachable_code)]
            Ok(())
        });
        let mut failed = failed;
        let (completed, first) = joined_task(0, (&mut failed).await);
        let cleanup_done = Arc::new(AtomicBool::new(false));
        let observed = cleanup_done.clone();
        let capture = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            observed.store(true, Ordering::SeqCst);
            Ok(())
        });
        let result = finish_session_tasks(vec![failed, capture], 1, completed, first).await;
        assert!(cleanup_done.load(Ordering::SeqCst));
        assert!(result.unwrap_err().is::<crate::UnverifiedTeardown>());
    }
    #[tokio::test]
    async fn normal_completion_waits_for_capture_cleanup_before_opening_the_gate() {
        let cleanup_done = Arc::new(AtomicBool::new(false));
        let observed = cleanup_done.clone();
        let capture = tokio::spawn(async move {
            tokio::task::yield_now().await;
            observed.store(true, Ordering::SeqCst);
            Ok(())
        });
        finish_session_tasks(vec![capture], 0, None, Ok(()))
            .await
            .unwrap();
        assert!(cleanup_done.load(Ordering::SeqCst));
    }
    #[test]
    fn cancellation_after_callback_locks_input_prevents_the_delayed_operation() {
        let stop = CancellationToken::new();
        let worker_stop = stop.clone();
        let input = Arc::new(Mutex::new(InputGuard::default()));
        let worker_input = input.clone();
        let (locked, lock_seen) = std::sync::mpsc::sync_channel(0);
        let (proceed, resume) = std::sync::mpsc::sync_channel(0);
        let worker = std::thread::spawn(move || {
            let mut input = worker_input.lock().unwrap();
            locked.send(()).unwrap();
            resume.recv().unwrap();
            apply_control_locked(
                &mut input,
                &worker_stop,
                &crate::protocol::ControlMessage::ReleaseAll,
                PermissionSet::default(),
            )
        });
        lock_seen.recv_timeout(Duration::from_secs(1)).unwrap();
        stop.cancel();
        proceed.send(()).unwrap();
        assert!(worker.join().unwrap().is_err());
        // The teardown owner can still release directly after obtaining the lock.
        input.lock().unwrap().release_all().unwrap();
    }
}
