#[cfg(not(windows))]
use anyhow::bail;
use anyhow::Result;
#[cfg(windows)]
use openremote_agent::{api::Api, config::Config, identity::DeviceIdentity, session};
#[cfg(windows)]
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
#[cfg(windows)]
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(not(windows))]
    {
        bail!("The desktop application runs on Windows; use cargo test or examples/synthetic_peer for Linux verification");
    }
    #[cfg(windows)]
    {
        openremote_agent::displays::initialize_dpi_awareness()?;
        run().await
    }
}

#[cfg(windows)]
async fn run() -> Result<()> {
    let cfg = Config::load_or_create().await?;
    let _instance = acquire_instance_lock()?;
    let identity = DeviceIdentity::load_or_create().await?;
    let api = Api::enroll_or_load(&cfg, &identity.public_key_b64()).await?;
    println!("OpenRemote host — PC ID: {}\nKeep the host home window running. Closing it or pressing Ctrl+C stops all remote access.\nScope: one selected display in the current user's desktop. UAC/lock screen are not supported.",api.display_id());
    let stop = CancellationToken::new();
    let home = openremote_agent::host_home::HostPanel::show(api.display_id(), &cfg, stop.clone())?;
    if let Ok(link) =
        openremote_agent::host_home::phone_setup_link(&cfg.api_base_url, api.display_id())
    {
        println!("Phone setup link (address only; PC approval still required):\n{link}");
    }
    let shutdown = stop.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        shutdown.cancel();
    });
    let hb_api = api.clone();
    let hb_stop = stop.clone();
    let delay = Duration::from_secs(cfg.heartbeat_seconds);
    let heartbeat = tokio::spawn(async move {
        loop {
            if hb_api.heartbeat().await.is_err() {
                eprintln!("Control-plane heartbeat unavailable; retrying");
            }
            tokio::select! {_ = hb_stop.cancelled()=>break,_ = tokio::time::sleep(delay)=>{}}
        }
    });
    let busy = Arc::new(AtomicBool::new(false));
    let restart_required = Arc::new(AtomicBool::new(false));
    let mut active: Option<tokio::task::JoinHandle<()>> = None;
    let mut active_id: Option<String> = None;
    let mut active_stop: Option<CancellationToken> = None;
    let mut seq = 0;
    let mut backoff = 1;
    loop {
        let events = tokio::select! {_ = stop.cancelled()=>break,r=api.events(seq)=>r};
        let events = match events {
            Ok(events) => {
                backoff = 1;
                events
            }
            Err(error) => {
                if let Some(failure) = error.downcast_ref::<openremote_agent::api::ApiFailure>() {
                    if failure.code == "cursor_reset_required" {
                        seq = 0;
                        continue;
                    }
                    if failure.code == "cursor_expired" {
                        if let Some(reset) = failure.reset_after {
                            seq = reset;
                            continue;
                        }
                    }
                    if failure.status == 401 || failure.status == 403 {
                        stop.cancel();
                        eprintln!(
                            "Host enrollment was revoked; explicit owner re-enrollment required."
                        );
                        break;
                    }
                }
                eprintln!("Host event channel unavailable; retrying in {backoff}s");
                tokio::select! {_ = stop.cancelled()=>break,_ = tokio::time::sleep(Duration::from_secs(backoff))=>{}}
                backoff = (backoff * 2).min(30);
                continue;
            }
        };
        for event in events {
            seq = seq.max(event.seq);
            if event.event_type != "session.requested" {
                continue;
            }
            let Some(id) = event.session_id else {
                continue;
            };
            let lookup = tokio::select! {
                _=stop.cancelled()=>break,
                r=tokio::time::timeout(Duration::from_secs(5),api.session(&id))=>r,
            };
            let request = match lookup {
                Ok(Ok(v)) => v,
                result => {
                    if let Ok(Err(error)) = &result {
                        if let Some(failure) =
                            error.downcast_ref::<openremote_agent::api::ApiFailure>()
                        {
                            if matches!(failure.status, 404 | 410) {
                                continue;
                            }
                            if matches!(failure.status, 401 | 403) {
                                stop.cancel();
                                break;
                            }
                        }
                    }
                    // A failed status lookup has not handled this event. Keep
                    // it replayable; otherwise a transient 500/timeout silently
                    // loses the only connection request from the phone.
                    seq = event.seq.saturating_sub(1);
                    tokio::select! {_=stop.cancelled()=>{},_=tokio::time::sleep(Duration::from_secs(1))=>{}}
                    break;
                }
            };
            // /end is acknowledged before the controller requests another
            // display. The old host task may still be reaping its encoder. Give
            // that terminal session bounded cleanup time instead of racing a
            // busy rejection or allowing two local sessions to overlap.
            if request.status == "pending" && restart_required.load(Ordering::SeqCst) {
                let _ = api.action(&id, "reject").await;
                continue;
            }
            if request.status == "pending" && busy.load(Ordering::SeqCst) {
                if let (Some(old_id), Some(old_stop), Some(old_task)) =
                    (active_id.as_deref(), active_stop.as_ref(), active.as_mut())
                {
                    if old_id != request.id {
                        if let Ok(Ok(old)) =
                            tokio::time::timeout(Duration::from_secs(5), api.session(old_id)).await
                        {
                            match session::finish_terminal_session(
                                &old.status,
                                old_stop,
                                old_task,
                                Duration::from_secs(8),
                            )
                            .await
                            {
                                session::TeardownWait::Finished => {
                                    if !restart_required.load(Ordering::SeqCst) {
                                        busy.store(false, Ordering::SeqCst);
                                    }
                                    active = None;
                                    active_id = None;
                                    active_stop = None;
                                }
                                session::TeardownWait::Failed => {
                                    // A panicked task cannot establish successful
                                    // cleanup. Keep the host busy until restart,
                                    // and never poll a consumed JoinHandle again.
                                    active = None;
                                    restart_required.store(true, Ordering::SeqCst);
                                    eprintln!("Session teardown could not be verified; restart the host before connecting again.");
                                }
                                session::TeardownWait::StillActive
                                | session::TeardownWait::TimedOut => {}
                            }
                        }
                    }
                }
            }
            if request.status == "pending" && restart_required.load(Ordering::SeqCst) {
                let _ = api.action(&id, "reject").await;
                continue;
            }
            let running_id = if busy.load(Ordering::SeqCst) {
                active_id.as_deref()
            } else {
                None
            };
            match session::request_decision(&request, running_id) {
                session::RequestDecision::Ignore => continue,
                session::RequestDecision::RejectBusy => {
                    let _ = api.action(&id, "reject").await;
                    continue;
                }
                session::RequestDecision::EndOrphan => {
                    // The per-user instance lock proves there is no second local
                    // host process serving this retained pre-restart session.
                    if !matches!(
                        tokio::time::timeout(Duration::from_secs(5), api.action(&id, "end")).await,
                        Ok(Ok(()))
                    ) {
                        // Keep this event replayable if the coordinator is briefly
                        // unavailable, instead of leaving the PC permanently busy.
                        seq = event.seq.saturating_sub(1);
                        tokio::select! {_=stop.cancelled()=>{},_=tokio::time::sleep(Duration::from_secs(1))=>{}}
                        break;
                    }
                    continue;
                }
                session::RequestDecision::Start => {}
            }
            busy.store(true, Ordering::SeqCst);
            active_id = Some(id.clone());
            let a = api.clone();
            let c = cfg.clone();
            let s = stop.child_token();
            active_stop = Some(s.clone());
            let b = busy.clone();
            let restart = restart_required.clone();
            active = Some(tokio::spawn(async move {
                let mut reusable = true;
                if let Err(error) = session::run(a.clone(), c, request, s).await {
                    reusable = !error.is::<openremote_agent::UnverifiedTeardown>();
                    if !reusable {
                        restart.store(true, Ordering::SeqCst);
                    }
                    eprintln!("Remote session stopped: {error:#}");
                }
                let _ = tokio::time::timeout(Duration::from_secs(3), a.action(&id, "end")).await;
                if reusable {
                    b.store(false, Ordering::SeqCst);
                }
            }));
        }
    }
    stop.cancel();
    heartbeat.abort();
    if let Some(mut task) = active {
        if tokio::time::timeout(Duration::from_secs(8), &mut task)
            .await
            .is_err()
        {
            task.abort();
            let _ = tokio::time::timeout(Duration::from_secs(1), task).await;
        }
    }
    home.close()
}

#[cfg(windows)]
fn acquire_instance_lock() -> Result<std::fs::File> {
    use anyhow::Context;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT,
    };
    let file=std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false)
        .share_mode(0).custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(openremote_agent::config::data_dir().join("host-instance.lock"))
        .context("OpenRemote is already running for this Windows user, or its instance lock is unavailable")?;
    anyhow::ensure!(
        file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
        "invalid host instance lock"
    );
    Ok(file)
}
