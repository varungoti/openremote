//! Local phone setup and the host's persistent owner-visible home window.
//!
//! A setup link is only an address. It never contains an enrollment token, host
//! bearer, controller key or permission grant, and does not authorize a session.
use anyhow::{bail, Context, Result};
use tokio_util::sync::CancellationToken;

pub const MAX_SETUP_LINK_BYTES: usize = 2048;

/// Build the receiving-PC entry point from the already configured origin.
/// The fixed path cannot introduce a query, credentials, PIN or shell command.
pub fn browser_viewer_link(endpoint: &str) -> Result<String> {
    phone_setup_link(endpoint, "000000001")?;
    let origin = reqwest::Url::parse(endpoint)?
        .origin()
        .ascii_serialization();
    Ok(format!("{origin}/connect"))
}

pub fn phone_setup_link(endpoint: &str, pc_number: &str) -> Result<String> {
    // Check the raw origin before URL parsers can repair slashes, remove
    // controls, resolve /a/.., or percent-decode an ambiguous authority.
    let raw_authority = endpoint
        .get(8..)
        .unwrap_or("")
        .strip_suffix('/')
        .unwrap_or_else(|| endpoint.get(8..).unwrap_or(""));
    if !endpoint.is_ascii()
        || !endpoint
            .get(..8)
            .is_some_and(|v| v.eq_ignore_ascii_case("https://"))
        || endpoint
            .bytes()
            .any(|c| c.is_ascii_whitespace() || c.is_ascii_control())
        || endpoint.contains(['\\', '%', '+', '@', '?', '#'])
        || raw_authority.is_empty()
        || raw_authority.contains('/')
    {
        bail!("invalid server origin for phone setup");
    }
    let origin = reqwest::Url::parse(endpoint).context("invalid phone setup server origin")?;
    if origin.scheme() != "https"
        || origin.host().is_none()
        || !origin.username().is_empty()
        || origin.password().is_some()
        || endpoint.contains('@')
        || origin.path() != "/"
        || origin.query().is_some()
        || origin.fragment().is_some()
    {
        bail!("phone setup requires an HTTPS origin without credentials, path, query or fragment");
    }
    let host = origin
        .host_str()
        .context("phone setup server host is missing")?;
    let dns = host.strip_suffix('.').unwrap_or(host);
    let ip = host
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .is_some_and(|v| v.parse::<std::net::Ipv6Addr>().is_ok())
        || host.parse::<std::net::Ipv4Addr>().is_ok();
    let valid_dns = !dns.is_empty()
        && dns.len() <= 253
        && dns.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.as_bytes()[0].is_ascii_alphanumeric()
                && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                && label
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        });
    if !ip && !valid_dns {
        bail!("phone setup requires a valid ASCII DNS or IP host")
    }
    if pc_number.len() != 9 || !pc_number.bytes().all(|c| c.is_ascii_digit()) {
        bail!("phone setup requires a nine-digit PC number");
    }
    let mut link = reqwest::Url::parse("openremote://connect")?;
    link.query_pairs_mut()
        .append_pair("v", "1")
        .append_pair("server", &origin.origin().ascii_serialization())
        .append_pair("pc", pc_number);
    let link = link.to_string();
    if !link.is_ascii() || link.len() > MAX_SETUP_LINK_BYTES {
        bail!("phone setup link exceeds the supported size");
    }
    Ok(link)
}

#[cfg(windows)]
static PANIC_STOP: std::sync::OnceLock<CancellationToken> = std::sync::OnceLock::new();

#[cfg(windows)]
pub(crate) fn install_panic_stop(stop: CancellationToken) {
    let _ = PANIC_STOP.set(stop);
}

/// A panic hook runs before unwinding and may show a blocking native alert.
/// Cancel access first without touching UI state or acquiring its RefCell.
#[cfg(windows)]
pub fn revoke_for_panic() {
    if let Some(stop) = PANIC_STOP.get() {
        stop.cancel();
    }
}

/// Cancels access on normal UI exit, an error, or an unwinding UI thread.
/// The owner thread installs this before attempting any native resource work.
pub(crate) struct RevokeOnExit(pub(crate) CancellationToken);
impl Drop for RevokeOnExit {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// An attempt may panic after starting detached session/heartbeat children.
/// Cancel their shared token before publishing any terminal UI result, and
/// also when the owning task itself is aborted or unwinds.
#[cfg(any(windows, test))]
pub(crate) async fn supervised_attempt<F, T>(
    stop: CancellationToken,
    work: F,
) -> std::result::Result<T, tokio::task::JoinError>
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let guard = RevokeOnExit(stop);
    let result = tokio::spawn(work).await;
    drop(guard);
    result
}

#[cfg(any(windows, test))]
pub(crate) fn fit_axis(
    start: i32,
    length: i32,
    work_start: i32,
    work_end: i32,
) -> Result<(i32, i32)> {
    let extent = work_end
        .checked_sub(work_start)
        .context("invalid work area")?;
    if extent <= 0 || length <= 0 {
        bail!("invalid window dimensions")
    }
    let size = length.min(extent);
    Ok((start.clamp(work_start, work_end - size), size))
}

#[cfg(any(windows, test))]
pub(crate) fn reveal_axis(offset: i32, viewport: i32, start: i32, extent: i32) -> i32 {
    if start < offset || extent >= viewport {
        start
    } else if start + extent > offset + viewport {
        start + extent - viewport
    } else {
        offset
    }
}

#[cfg(any(windows, test))]
pub use runtime::{serve, HostOutcome};

#[cfg(any(windows, test))]
mod runtime {
    use crate::{
        api::Api, config::Config, connections::Connections, pairing::PairingGate, session,
    };
    use anyhow::{ensure, Result};
    use std::{
        collections::BTreeMap,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Mutex,
        },
        time::Duration,
    };
    use tokio_util::sync::CancellationToken;

    #[derive(Clone, Copy, Debug)]
    pub enum HostOutcome {
        Stopped,
        Expired,
    }

    struct Active {
        stop: CancellationToken,
        task: tokio::task::JoinHandle<Result<()>>,
    }

    /// The activation survives individual sessions. Every native resource owner
    /// remains in this bounded map until teardown has been observed.
    pub async fn serve(
        cfg: Config,
        api: Api,
        pairing: PairingGate,
        stop: CancellationToken,
        connections: Connections,
        restart_required: Arc<AtomicBool>,
    ) -> Result<HostOutcome> {
        let _cancel_on_exit = super::RevokeOnExit(stop.clone());
        let initial = pairing
            .snapshot()
            .ok_or_else(|| anyhow::anyhow!("Activation is unavailable."))?;
        ensure!(initial.code.is_activation(), "Activation is unavailable.");
        let check_api = api.clone();
        let check_stop = stop.clone();
        let check_gate = pairing.clone();
        let heartbeat_delay = Duration::from_secs(cfg.heartbeat_seconds);
        let mut maintenance: tokio::task::JoinHandle<Result<bool>> = tokio::spawn(async move {
            let _cancel_on_exit = super::RevokeOnExit(check_stop.clone());
            let mut failures = 0;
            let mut next_heartbeat = tokio::time::Instant::now();
            loop {
                let result = tokio::select! {
                    biased; _ = check_stop.cancelled() => return Ok(false),
                    result = tokio::time::timeout(Duration::from_secs(5), check_api.current_pairing()) => result,
                };
                match result {
                    Ok(Ok(Some(current)))
                        if current.is_activation()
                            && current.pairing_id == initial.code.pairing_id
                            && current.code == initial.code.code
                            && current.permissions == initial.code.permissions
                            && current.expires_at == initial.code.expires_at =>
                    {
                        failures = 0
                    }
                    Ok(Ok(_)) => {
                        check_gate.close();
                        return Ok(true);
                    }
                    Ok(Err(_)) | Err(_) => {
                        failures += 1;
                        ensure!(failures < 3, "The service could not refresh access. Activate again after your connection recovers.");
                    }
                }
                if tokio::time::Instant::now() >= next_heartbeat {
                    let heartbeat = tokio::select! { biased; _=check_stop.cancelled()=>return Ok(false), result=tokio::time::timeout(Duration::from_secs(5), check_api.heartbeat())=>result };
                    if let Ok(Err(error)) = heartbeat {
                        if error
                            .downcast_ref::<crate::api::ApiFailure>()
                            .is_some_and(|failure| matches!(failure.status, 401 | 403))
                        {
                            anyhow::bail!("This PC's registration is no longer authorized. Contact the app provider.");
                        }
                    }
                    next_heartbeat = tokio::time::Instant::now() + heartbeat_delay;
                }
                tokio::select! { biased; _=check_stop.cancelled()=>return Ok(false), _=tokio::time::sleep(Duration::from_secs(2))=>{} }
            }
        });
        let mut active: BTreeMap<String, Active> = BTreeMap::new();
        let session_failure = Arc::new(Mutex::new(None::<&'static str>));
        let mut seq = 0;
        let mut backoff = 1;
        let mut outcome = HostOutcome::Stopped;
        let mut failure: Option<anyhow::Error> = None;
        'hosting: loop {
            let done = active
                .iter()
                .filter(|(_, work)| work.task.is_finished())
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            for id in done {
                let work = active.remove(&id).expect("tracked session");
                match work.task.await {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) if !error.is::<crate::UnverifiedTeardown>() => {}
                    _ => {
                        restart_required.store(true, Ordering::SeqCst);
                        stop.cancel();
                    }
                }
            }
            if stop.is_cancelled() || restart_required.load(Ordering::SeqCst) {
                break;
            }
            if pairing.snapshot().is_none() {
                outcome = HostOutcome::Expired;
                break;
            }
            let polling = api.events(seq);
            tokio::pin!(polling);
            let events = loop {
                tokio::select! {
                    biased;
                    _ = stop.cancelled() => break 'hosting,
                    result = &mut polling => break result,
                    _ = tokio::time::sleep(Duration::from_millis(200)) => {
                        if restart_required.load(Ordering::SeqCst) { stop.cancel(); break 'hosting; }
                        if pairing.snapshot().is_none() { outcome = HostOutcome::Expired; break 'hosting; }
                        if maintenance.is_finished() { break 'hosting; }
                        if active.values().any(|work| work.task.is_finished()) { continue 'hosting; }
                    }
                }
            };
            let events = match events {
                Ok(events) => {
                    backoff = 1;
                    events
                }
                Err(error) => {
                    if let Some(api_failure) = error.downcast_ref::<crate::api::ApiFailure>() {
                        if api_failure.code == "cursor_reset_required" {
                            seq = 0;
                            continue;
                        }
                        if api_failure.code == "cursor_expired" {
                            if let Some(reset) = api_failure.reset_after {
                                seq = reset;
                                continue;
                            }
                        }
                        if matches!(api_failure.status, 401 | 403) {
                            failure = Some(anyhow::anyhow!(
                                "This PC's service registration is no longer authorized."
                            ));
                            break;
                        }
                    }
                    tokio::select! { biased; _=stop.cancelled()=>break, _=tokio::time::sleep(Duration::from_secs(backoff))=>{} }
                    backoff = (backoff * 2).min(5);
                    continue;
                }
            };
            for event in events {
                if stop.is_cancelled() || pairing.snapshot().is_none() {
                    break 'hosting;
                }
                if active.values().any(|work| work.task.is_finished()) {
                    continue 'hosting;
                }
                seq = seq.max(event.seq);
                if event.event_type != "session.requested" {
                    continue;
                }
                let Some(id) = event.session_id else {
                    continue;
                };
                if active.contains_key(&id) {
                    continue;
                }
                let lookup = tokio::select! { biased; _=stop.cancelled()=>break 'hosting, r=tokio::time::timeout(Duration::from_secs(5),api.session(&id))=>r };
                let request = match lookup {
                    Ok(Ok(value)) => value,
                    result => {
                        if let Ok(Err(error)) = &result {
                            if let Some(api_failure) =
                                error.downcast_ref::<crate::api::ApiFailure>()
                            {
                                if matches!(api_failure.status, 404 | 410) {
                                    continue;
                                }
                                if matches!(api_failure.status, 401 | 403) {
                                    failure = Some(anyhow::anyhow!(
                                        "This PC's service registration is no longer authorized."
                                    ));
                                    break 'hosting;
                                }
                            }
                        }
                        seq = event.seq.saturating_sub(1);
                        tokio::select! { _=stop.cancelled()=>{}, _=tokio::time::sleep(Duration::from_secs(1))=>{} }
                        break;
                    }
                };
                if request.status != "pending" {
                    if matches!(
                        request.status.as_str(),
                        "approved" | "connecting" | "active"
                    ) {
                        // The per-user process lock proves this retained session is orphaned.
                        if !matches!(
                            tokio::time::timeout(Duration::from_secs(3), api.action(&id, "end"))
                                .await,
                            Ok(Ok(()))
                        ) {
                            seq = event.seq.saturating_sub(1);
                            break;
                        }
                    }
                    continue;
                }
                if request.mode.as_deref() != Some("activation")
                    || !pairing.matches_pending(&request)
                    || restart_required.load(Ordering::SeqCst)
                {
                    let _ = tokio::time::timeout(Duration::from_secs(3), api.action(&id, "reject"))
                        .await;
                    continue;
                }
                if active.len() >= crate::connections::MAX_CONNECTIONS {
                    let _ = tokio::time::timeout(Duration::from_secs(3), api.action(&id, "reject"))
                        .await;
                    continue;
                }
                let Some(listing) = connections.reserve(
                    &id,
                    &request.controller_name,
                    request.permissions.control_input,
                ) else {
                    let _ = tokio::time::timeout(Duration::from_secs(3), api.action(&id, "reject"))
                        .await;
                    continue;
                };
                let child_stop = stop.child_token();
                let session_stop = child_stop.clone();
                let session_api = api.clone();
                let session_config = cfg.clone();
                let grant = pairing.clone();
                let session_id = id.clone();
                let session_restart = restart_required.clone();
                let host_stop = stop.clone();
                let end_failure = session_failure.clone();
                let task = tokio::spawn(async move {
                    let _cancel_on_exit = super::RevokeOnExit(session_stop.clone());
                    let result = session::run_with_owner_window(
                        session_api.clone(),
                        session_config,
                        request,
                        session_stop,
                        grant,
                        listing.monitor(),
                    )
                    .await;
                    if result
                        .as_ref()
                        .err()
                        .is_some_and(|error| error.is::<crate::UnverifiedTeardown>())
                    {
                        session_restart.store(true, Ordering::SeqCst);
                        host_stop.cancel();
                    }
                    let ended = tokio::time::timeout(
                        Duration::from_secs(3),
                        session_api.action(&session_id, "end"),
                    )
                    .await;
                    if !matches!(ended, Ok(Ok(()))) && !host_stop.is_cancelled() {
                        if let Ok(mut reason) = end_failure.lock() {
                            *reason = Some("The service could not confirm a connection ended. Activate Connection again to reset access.");
                        }
                        host_stop.cancel();
                    }
                    drop(listing);
                    result
                });
                active.insert(
                    id,
                    Active {
                        stop: child_stop,
                        task,
                    },
                );
            }
        }
        stop.cancel();
        pairing.close();
        match tokio::time::timeout(Duration::from_secs(1), &mut maintenance).await {
            Ok(Ok(Err(error))) => failure = Some(error),
            Ok(Err(_)) => restart_required.store(true, Ordering::SeqCst),
            Ok(Ok(Ok(grant_ended))) => {
                if grant_ended {
                    outcome = HostOutcome::Expired;
                }
            }
            Err(_) => {
                maintenance.abort();
                let _ = maintenance.await;
            }
        }
        // All children are canceled first; cleanup waits happen concurrently so
        // four sessions do not multiply the bounded owner-stop latency.
        for work in active.values() {
            work.stop.cancel();
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(9);
        for (_, mut work) in active {
            match tokio::time::timeout_at(deadline, &mut work.task).await {
                Ok(Ok(Ok(()))) => {}
                Ok(Ok(Err(error))) if !error.is::<crate::UnverifiedTeardown>() => {}
                _ => {
                    restart_required.store(true, Ordering::SeqCst);
                    work.task.abort();
                }
            }
        }
        connections.clear();
        if let Ok(reason) = session_failure.lock() {
            if let Some(reason) = *reason {
                failure = Some(anyhow::anyhow!(reason));
            }
        }
        ensure!(
            !restart_required.load(Ordering::SeqCst),
            "Restart the app before activating again."
        );
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn owner_abort_revokes_all_four_sessions_and_clears_the_live_roster() {
        let stop = CancellationToken::new();
        let connections = crate::connections::Connections::default();
        let listing = connections.clone();
        let parent = stop.clone();
        let (started, ready) = tokio::sync::oneshot::channel();
        let (finished, mut observed) = tokio::sync::mpsc::channel(4);
        let owner = tokio::spawn(supervised_attempt(stop.clone(), async move {
            for n in 0..4 {
                let entry = listing
                    .reserve(&format!("session-{n}"), "Device", n == 0)
                    .unwrap();
                entry
                    .monitor()
                    .set_state(crate::connections::ConnectionState::Connected);
                let child = parent.child_token();
                let finished = finished.clone();
                tokio::spawn(async move {
                    child.cancelled().await;
                    drop(entry);
                    let _ = finished.send(()).await;
                });
            }
            let _ = started.send(());
            std::future::pending::<()>().await;
        }));
        ready.await.unwrap();
        assert_eq!(connections.snapshot().connected_count(), 4);
        owner.abort();
        assert!(owner.await.unwrap_err().is_cancelled());
        for _ in 0..4 {
            tokio::time::timeout(std::time::Duration::from_secs(1), observed.recv())
                .await
                .unwrap()
                .unwrap();
        }
        assert!(stop.is_cancelled());
        assert!(connections.snapshot().devices.is_empty());
    }

    #[tokio::test]
    async fn completing_one_session_keeps_its_sibling_and_activation_alive() {
        let stop = CancellationToken::new();
        let sibling = stop.child_token();
        let first = stop.child_token();
        let child = first.child_token();
        {
            let _guard = RevokeOnExit(first);
        }
        assert!(child.is_cancelled());
        assert!(!sibling.is_cancelled());
        assert!(!stop.is_cancelled());
    }

    #[tokio::test]
    async fn attempt_panic_cancels_detached_children_before_reporting_failure() {
        let stop = CancellationToken::new();
        let child = stop.child_token();
        let (finished, observed) = tokio::sync::oneshot::channel();
        let result = supervised_attempt(stop.clone(), async move {
            tokio::spawn(async move {
                child.cancelled().await;
                let _ = finished.send(());
            });
            panic!("synthetic attempt failure after spawning a child");
        })
        .await;
        assert!(result.unwrap_err().is_panic());
        assert!(stop.is_cancelled());
        tokio::time::timeout(std::time::Duration::from_secs(1), observed)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn aborting_the_owner_cancels_the_running_attempt() {
        let stop = CancellationToken::new();
        let child = stop.child_token();
        let (started, ready) = tokio::sync::oneshot::channel();
        let (finished, observed) = tokio::sync::oneshot::channel();
        let owner = tokio::spawn(supervised_attempt(stop.clone(), async move {
            let _ = started.send(());
            child.cancelled().await;
            let _ = finished.send(());
        }));
        ready.await.unwrap();
        owner.abort();
        assert!(owner.await.unwrap_err().is_cancelled());
        assert!(stop.is_cancelled());
        tokio::time::timeout(std::time::Duration::from_secs(1), observed)
            .await
            .unwrap()
            .unwrap();
    }

    #[test]
    fn browser_viewer_uses_only_the_configured_https_origin_and_fixed_path() {
        assert_eq!(
            browser_viewer_link("https://remote.owner.dev").unwrap(),
            "https://remote.owner.dev/connect"
        );
        assert_eq!(
            browser_viewer_link("HTTPS://REMOTE.owner.dev:443/").unwrap(),
            "https://remote.owner.dev/connect"
        );
        assert_eq!(
            browser_viewer_link("https://[2001:db8::1]:8443").unwrap(),
            "https://[2001:db8::1]:8443/connect"
        );
        for value in [
            "http://localhost",
            "javascript:alert(1)",
            "file:///C:/Windows/cmd.exe",
            "https://user:pin@remote.owner.dev",
            "https://remote.owner.dev/another-page",
            "https://remote.owner.dev/?pin=123456",
            "https://remote.owner.dev/#123456",
            "https://remote.owner.dev\\other",
            "https://remote.owner.dev\n",
            "https://remote.owner.dev/../",
        ] {
            assert!(browser_viewer_link(value).is_err(), "{value:?}");
        }
    }

    #[test]
    fn setup_link_matches_phone_wire_contract_without_credentials() {
        assert_eq!(
            phone_setup_link("https://remote.example.com", "123456789").unwrap(),
            "openremote://connect?v=1&server=https%3A%2F%2Fremote.example.com&pc=123456789"
        );
        assert_eq!(
            phone_setup_link("HTTPS://REMOTE.example.com:443/", "123456789").unwrap(),
            "openremote://connect?v=1&server=https%3A%2F%2Fremote.example.com&pc=123456789"
        );
        assert_eq!(
            phone_setup_link("https://remote.example.com:8443", "123456789").unwrap(),
            "openremote://connect?v=1&server=https%3A%2F%2Fremote.example.com%3A8443&pc=123456789"
        );
        assert_eq!(phone_setup_link("https://[2001:db8::1]:8443", "123456789").unwrap(),
            "openremote://connect?v=1&server=https%3A%2F%2F%5B2001%3Adb8%3A%3A1%5D%3A8443&pc=123456789");
        let link = phone_setup_link("https://[2001:db8::1]:8443", "000000001").unwrap();
        let parsed = reqwest::Url::parse(&link).unwrap();
        assert_eq!(parsed.path(), "");
        assert_eq!(
            parsed.query_pairs().collect::<Vec<_>>(),
            vec![
                ("v".into(), "1".into()),
                ("server".into(), "https://[2001:db8::1]:8443".into()),
                ("pc".into(), "000000001".into())
            ]
        );
    }

    #[test]
    fn setup_rejects_insecure_credentialed_or_non_origin_addresses() {
        for endpoint in [
            "http://localhost",
            "https://user:secret@example.com",
            "https://@example.com",
            "https://remote.example.com/v1",
            "https://remote.example.com/a/..",
            "https:remote.example.com",
            "https:/remote.example.com",
            "https://remote.example.com//",
            "https://%72emote.example.com",
            "https://remote.example.com\\",
            "https://remote+example.com",
            "https://éxample.com",
            "https://remote_server.example",
            "https://-remote.example",
            "https://remote-.example",
            "https://remote..example",
            "https://remote.example.com?token=secret",
            "https://remote.example.com#x",
            " https://remote.example.com",
            "https://remo\nte.example.com",
            "file:///tmp/server",
            "",
        ] {
            assert!(
                phone_setup_link(endpoint, "123456789").is_err(),
                "{endpoint:?}"
            );
        }
    }

    #[test]
    fn setup_requires_exact_bounded_pc_identity_and_output() {
        for pc in [
            "12345678",
            "1234567890",
            "123 456 789",
            "123-456-789",
            "１２３４５６７８９",
            "12345678&",
            "12345678\n",
        ] {
            assert!(
                phone_setup_link("https://remote.example.com", pc).is_err(),
                "{pc:?}"
            );
        }
        assert!(phone_setup_link(
            &format!("https://{}.example.com", "a".repeat(MAX_SETUP_LINK_BYTES)),
            "123456789"
        )
        .is_err());
    }

    #[test]
    fn home_thread_exit_revokes_host_and_session_tokens() {
        let stop = CancellationToken::new();
        let session = stop.child_token();
        let thread_stop = stop.clone();
        std::thread::spawn(move || {
            let _guard = RevokeOnExit(thread_stop);
        })
        .join()
        .unwrap();
        assert!(stop.is_cancelled());
        assert!(session.is_cancelled());
    }

    #[test]
    fn home_thread_panic_also_revokes_access() {
        let stop = CancellationToken::new();
        let session = stop.child_token();
        let thread_stop = stop.clone();
        assert!(std::thread::spawn(move || {
            let _guard = RevokeOnExit(thread_stop);
            panic!("synthetic UI failure");
        })
        .join()
        .is_err());
        assert!(stop.is_cancelled());
        assert!(session.is_cancelled());
    }

    #[test]
    fn host_home_fits_small_and_negative_origin_work_areas() {
        // A 150% scaled 780x590 window on a 1280x720 monitor with a taskbar.
        assert_eq!(fit_axis(50, 1170, 0, 1280).unwrap(), (50, 1170));
        assert_eq!(fit_axis(30, 885, 0, 680).unwrap(), (0, 680));
        assert_eq!(fit_axis(-2100, 1560, -1920, 0).unwrap(), (-1920, 1560));
        assert_eq!(fit_axis(-200, 780, -1920, 0).unwrap(), (-780, 780));
        assert!(fit_axis(0, 100, 20, 20).is_err());
    }

    #[test]
    fn tab_focus_reveals_offscreen_fields_without_moving_visible_ones() {
        assert_eq!(reveal_axis(0, 200, 206, 72), 78);
        assert_eq!(reveal_axis(200, 200, 20, 100), 20);
        assert_eq!(reveal_axis(180, 200, 206, 72), 180);
        assert_eq!(reveal_axis(300, 200, 20, 700), 20);
    }
}
