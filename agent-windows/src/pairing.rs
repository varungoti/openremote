//! Expiring owner authorizations: legacy single use or bounded active hosting.
//! Nothing here is persisted. A coordinator response alone cannot recreate a
//! local grant after restart, cancellation or replacement.
use crate::{api::SessionInfo, protocol::PermissionSet};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub const MAX_CODE_LIFETIME: Duration = Duration::from_secs(600);
pub const MAX_ACTIVATION_LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_SESSIONS_PER_ACTIVATION: usize = 4096;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PairingCode {
    pub pairing_id: String,
    pub code: String,
    pub expires_at: String,
    pub permissions: PermissionSet,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub max_connections: Option<usize>,
}

pub fn valid_pin(value: &str) -> bool {
    (6..=12).contains(&value.len()) && value.bytes().all(|c| c.is_ascii_digit())
}

/// The simple 0.7 owner screen always uses exactly six digits; the earlier API
/// validator remains available for compatibility with existing integrations.
pub fn valid_simple_pin(value: &str) -> bool {
    value.len() == 6 && value.bytes().all(|c| c.is_ascii_digit())
}

pub fn validate_pin_confirmation(pin: &str, confirm: &str) -> Result<()> {
    ensure!(valid_pin(pin), "Choose a PIN containing 6 to 12 digits.");
    ensure!(pin == confirm, "The two PIN entries do not match.");
    Ok(())
}

pub fn pairing_permissions(allow_input: bool) -> PermissionSet {
    PermissionSet {
        view_screen: true,
        control_input: allow_input,
        ..Default::default()
    }
}

fn valid_pairing_id(id: &str) -> bool {
    (16..=64).contains(&id.len())
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

fn bounded_permissions(value: PermissionSet, ceiling: PermissionSet) -> bool {
    ceiling.view_screen
        && value.view_screen
        && !value.clipboard
        && !value.file_transfer
        && !value.audio
        && !value.power
        && (!value.control_input || ceiling.control_input)
}

impl PairingCode {
    pub fn is_activation(&self) -> bool {
        self.mode.as_deref() == Some("activation")
            && self.max_connections == Some(crate::connections::MAX_CONNECTIONS)
    }
    fn lifetime(&self, requested: PermissionSet, now: SystemTime) -> Result<Duration> {
        ensure!(valid_pairing_id(&self.pairing_id), "Invalid code response.");
        ensure!(
            self.code.len() == 6 && self.code.bytes().all(|c| c.is_ascii_digit()),
            "Invalid code response."
        );
        ensure!(
            bounded_permissions(self.permissions, requested),
            "The server returned unsupported code permissions."
        );
        ensure!(self.expires_at.len() <= 40, "Invalid code expiration.");
        let expiration = OffsetDateTime::parse(&self.expires_at, &Rfc3339)
            .context("Invalid code expiration.")?;
        let seconds =
            u64::try_from(expiration.unix_timestamp()).context("Invalid code expiration.")?;
        let expires = UNIX_EPOCH
            .checked_add(Duration::new(seconds, expiration.nanosecond()))
            .context("Invalid code expiration.")?;
        let lifetime = expires
            .duration_since(now)
            .context("The code has already expired. Check this PC's clock.")?;
        let maximum = if self.is_activation() {
            MAX_ACTIVATION_LIFETIME
        } else {
            ensure!(
                self.mode.is_none() && self.max_connections.is_none(),
                "Unsupported code mode."
            );
            MAX_CODE_LIFETIME
        };
        // Wall-clock skew cannot extend the monotonic local maximum.
        ensure!(
            !lifetime.is_zero() && lifetime <= maximum + Duration::from_secs(30),
            "The code expiration is invalid. Check this PC's clock."
        );
        Ok(lifetime.min(maximum))
    }
}

struct LocalGrant {
    code: PairingCode,
    deadline: Instant,
}

#[derive(Default)]
struct State {
    generation: u64,
    closed: bool,
    grant: Option<LocalGrant>,
    seen: HashSet<String>,
    live: HashMap<String, bool>,
}

/// Clones share one gate. Poisoning fails closed; it never creates permission.
#[derive(Clone, Default)]
pub struct PairingGate(Arc<Mutex<State>>);

#[derive(Clone)]
pub struct PairingSnapshot {
    pub generation: u64,
    pub code: PairingCode,
    pub remaining_seconds: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PairingDecision {
    Ordinary,
    Authorized,
    Reject,
}

impl PairingGate {
    /// Invalidate before starting a request, not after an HTTP response.
    pub fn replace(&self) -> Result<u64> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Code access is unavailable. Restart the host."))?;
        ensure!(!state.closed, "The host is stopping.");
        state.grant = None;
        state.seen.clear();
        state.live.clear();
        state.generation = state
            .generation
            .checked_add(1)
            .context("Restart the host before generating another code.")?;
        Ok(state.generation)
    }

    /// The caller must install before making the six-digit code visible.
    pub fn install(
        &self,
        generation: u64,
        code: PairingCode,
        requested: PermissionSet,
    ) -> Result<bool> {
        self.install_at(
            generation,
            code,
            requested,
            SystemTime::now(),
            Instant::now(),
        )
    }

    fn install_at(
        &self,
        generation: u64,
        code: PairingCode,
        requested: PermissionSet,
        wall: SystemTime,
        monotonic: Instant,
    ) -> Result<bool> {
        let lifetime = code.lifetime(requested, wall)?;
        let deadline = monotonic
            .checked_add(lifetime)
            .context("Invalid code expiration.")?;
        let mut state = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Code access is unavailable. Restart the host."))?;
        if state.closed || generation != state.generation {
            return Ok(false);
        }
        state.grant = Some(LocalGrant { code, deadline });
        Ok(true)
    }

    pub fn clear_if_current(&self, generation: u64) {
        if let Ok(mut state) = self.0.lock() {
            if state.generation == generation {
                state.grant = None;
                state.live.clear();
            }
        }
    }

    pub fn close(&self) {
        if let Ok(mut state) = self.0.lock() {
            state.closed = true;
            state.grant = None;
            state.live.clear();
        }
    }

    pub fn snapshot(&self) -> Option<PairingSnapshot> {
        self.snapshot_at(Instant::now())
    }

    fn snapshot_at(&self, now: Instant) -> Option<PairingSnapshot> {
        let mut state = self.0.lock().ok()?;
        if state.closed {
            return None;
        }
        let grant = state.grant.as_ref()?;
        let Some(remaining) = grant
            .deadline
            .checked_duration_since(now)
            .filter(|d| !d.is_zero())
        else {
            state.grant = None;
            state.live.clear();
            return None;
        };
        Some(PairingSnapshot {
            generation: state.generation,
            code: grant.code.clone(),
            remaining_seconds: remaining
                .as_secs()
                .saturating_add(u64::from(remaining.subsec_nanos() != 0)),
        })
    }

    pub fn consume(&self, request: &SessionInfo) -> PairingDecision {
        self.consume_at(request, Instant::now())
    }

    /// Filter historical or unrelated pairing events before reserving the
    /// single active-session slot. The later consume remains authoritative.
    pub fn matches_pending(&self, request: &SessionInfo) -> bool {
        let Some(snapshot) = self.snapshot() else {
            return false;
        };
        request.status == "pending"
            && request.monitor_id.is_none()
            && request.pairing_id.as_deref() == Some(snapshot.code.pairing_id.as_str())
            && request.mode == snapshot.code.mode
            && bounded_permissions(request.permissions, snapshot.code.permissions)
    }

    /// Release capacity after the native session has fully torn down. Replay
    /// history remains bounded and retained until this activation is replaced.
    pub fn release(&self, request: &SessionInfo) {
        if let Ok(mut state) = self.0.lock() {
            if state
                .grant
                .as_ref()
                .is_some_and(|grant| request.pairing_id.as_deref() == Some(&grant.code.pairing_id))
            {
                state.live.remove(&request.id);
            }
        }
    }

    pub fn authorized_now(&self, request: &SessionInfo) -> bool {
        if request.mode.as_deref() != Some("activation") {
            return true;
        }
        let Ok(state) = self.0.lock() else {
            return false;
        };
        !state.closed
            && state.live.contains_key(&request.id)
            && state.grant.as_ref().is_some_and(|grant| {
                Instant::now() < grant.deadline
                    && grant.code.is_activation()
                    && request.pairing_id.as_deref() == Some(grant.code.pairing_id.as_str())
                    && bounded_permissions(request.permissions, grant.code.permissions)
            })
    }

    fn consume_at(&self, request: &SessionInfo, now: Instant) -> PairingDecision {
        let Some(id) = request.pairing_id.as_deref() else {
            return if request.mode.is_some() {
                PairingDecision::Reject
            } else {
                PairingDecision::Ordinary
            };
        };
        let Ok(mut state) = self.0.lock() else {
            return PairingDecision::Reject;
        };
        if state.closed
            || request.status != "pending"
            || !valid_pairing_id(id)
            || request.monitor_id.is_some()
        {
            return PairingDecision::Reject;
        }
        let Some(grant) = state.grant.as_ref() else {
            return PairingDecision::Reject;
        };
        if grant.code.pairing_id != id || request.mode != grant.code.mode {
            return PairingDecision::Reject;
        }
        let accepted = now < grant.deadline
            && bounded_permissions(request.permissions, grant.code.permissions);
        if grant.code.is_activation() {
            if !accepted
                || !(1..=128).contains(&request.id.len())
                || !request
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                || state.seen.contains(&request.id)
                || state.live.len() >= crate::connections::MAX_CONNECTIONS
                || (request.permissions.control_input && state.live.values().any(|input| *input))
            {
                return PairingDecision::Reject;
            }
            if state.seen.len() >= MAX_SESSIONS_PER_ACTIVATION {
                state.grant = None;
                state.live.clear();
                return PairingDecision::Reject;
            }
            state.seen.insert(request.id.clone());
            state
                .live
                .insert(request.id.clone(), request.permissions.control_input);
            PairingDecision::Authorized
        } else {
            // Preserve the legacy single-use behavior exactly.
            state.grant = None;
            if accepted {
                PairingDecision::Authorized
            } else {
                PairingDecision::Reject
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clocks() -> (SystemTime, Instant) {
        (
            UNIX_EPOCH + Duration::from_secs(1_800_000_000),
            Instant::now(),
        )
    }
    fn code() -> PairingCode {
        PairingCode {
            pairing_id: "one-session-grant-12345".into(),
            code: "001234".into(),
            expires_at: "2027-01-15T08:10:00Z".into(),
            permissions: pairing_permissions(true),
            mode: None,
            max_connections: None,
        }
    }
    fn request() -> SessionInfo {
        SessionInfo {
            id: "a-session-123456789".into(),
            status: "pending".into(),
            permissions: pairing_permissions(true),
            controller_name: "Phone".into(),
            controller_public_key: "key".into(),
            monitor_id: None,
            pairing_id: Some(code().pairing_id),
            mode: None,
            host_name: "Office".into(),
        }
    }
    #[test]
    fn pin_confirmation_is_exact_and_leading_zeroes_are_preserved() {
        assert!(validate_pin_confirmation("000001", "000001").is_ok());
        assert!(validate_pin_confirmation("012345678901", "012345678901").is_ok());
        for value in [
            "12345",
            "1234567890123",
            "123 456",
            "１２３４５６",
            "12345\n",
        ] {
            assert!(!valid_pin(value));
        }
        assert!(validate_pin_confirmation("000001", "000002").is_err());
    }
    #[test]
    fn exact_grant_authorizes_once_across_clones() {
        let gate = PairingGate::default();
        let (wall, now) = clocks();
        let generation = gate.replace().unwrap();
        assert!(gate
            .install_at(generation, code(), pairing_permissions(true), wall, now)
            .unwrap());
        assert_eq!(gate.snapshot_at(now).unwrap().code.code, "001234");
        assert_eq!(
            gate.clone().consume_at(&request(), now),
            PairingDecision::Authorized
        );
        assert_eq!(gate.consume_at(&request(), now), PairingDecision::Reject);
        assert!(gate.snapshot_at(now).is_none());
    }
    #[test]
    fn unknown_and_restarted_pairing_is_never_trusted() {
        let gate = PairingGate::default();
        let (wall, now) = clocks();
        assert_eq!(gate.consume_at(&request(), now), PairingDecision::Reject);
        let generation = gate.replace().unwrap();
        gate.install_at(generation, code(), pairing_permissions(true), wall, now)
            .unwrap();
        let mut wrong = request();
        wrong.pairing_id = Some("different-grant-12345".into());
        assert_eq!(gate.consume_at(&wrong, now), PairingDecision::Reject);
        assert!(gate.snapshot_at(now).is_some());
        wrong.pairing_id = None;
        assert_eq!(gate.consume_at(&wrong, now), PairingDecision::Ordinary);
    }
    #[test]
    fn historical_pairing_does_not_reserve_or_consume_the_current_grant() {
        let gate = PairingGate::default();
        let (wall, now) = clocks();
        let generation = gate.replace().unwrap();
        gate.install_at(generation, code(), pairing_permissions(true), wall, now)
            .unwrap();
        let mut old = request();
        old.pairing_id = Some("previous-grant-12345".into());
        assert!(!gate.matches_pending(&old));
        let mut expanded = request();
        expanded.permissions.clipboard = true;
        assert!(!gate.matches_pending(&expanded));
        assert!(gate.matches_pending(&request()));
        assert_eq!(
            gate.consume_at(&request(), now),
            PairingDecision::Authorized
        );
        assert!(!gate.matches_pending(&request()));
        assert_eq!(gate.consume_at(&request(), now), PairingDecision::Reject);
    }
    #[test]
    fn local_revocation_and_late_generation_fail_closed() {
        let gate = PairingGate::default();
        let (wall, now) = clocks();
        let first = gate.replace().unwrap();
        let second = gate.replace().unwrap();
        assert!(!gate
            .install_at(first, code(), pairing_permissions(true), wall, now)
            .unwrap());
        assert!(gate
            .install_at(second, code(), pairing_permissions(true), wall, now)
            .unwrap());
        gate.clear_if_current(first);
        assert!(gate.snapshot_at(now).is_some());
        gate.clear_if_current(second);
        assert_eq!(gate.consume_at(&request(), now), PairingDecision::Reject);
        gate.close();
        assert!(!gate
            .install_at(second, code(), pairing_permissions(true), wall, now)
            .unwrap());
        assert!(gate.replace().is_err());
    }
    #[test]
    fn expired_grant_and_expanded_permission_cannot_authorize() {
        let (wall, now) = clocks();
        for change in 0..6 {
            let gate = PairingGate::default();
            let generation = gate.replace().unwrap();
            gate.install_at(generation, code(), pairing_permissions(true), wall, now)
                .unwrap();
            let mut value = request();
            match change {
                0 => value.permissions.clipboard = true,
                1 => value.permissions.file_transfer = true,
                2 => value.permissions.audio = true,
                3 => value.permissions.power = true,
                4 => value.monitor_id = Some("a".repeat(64)),
                _ => value.status = "approved".into(),
            }
            assert_eq!(gate.consume_at(&value, now), PairingDecision::Reject);
        }
        let gate = PairingGate::default();
        let generation = gate.replace().unwrap();
        gate.install_at(generation, code(), pairing_permissions(true), wall, now)
            .unwrap();
        assert_eq!(
            gate.consume_at(&request(), now + MAX_CODE_LIFETIME),
            PairingDecision::Reject
        );
    }
    #[test]
    fn response_validation_rejects_oversized_expiry_or_unsupported_grants() {
        let (wall, now) = clocks();
        for change in 0..5 {
            let mut value = code();
            match change {
                0 => value.code = "12345".into(),
                1 => value.pairing_id = "short".into(),
                2 => value.expires_at = "2027-01-15T08:20:00Z".into(),
                3 => value.expires_at = "2027-01-15T07:59:00Z".into(),
                _ => value.permissions.clipboard = true,
            }
            let gate = PairingGate::default();
            let generation = gate.replace().unwrap();
            assert!(gate
                .install_at(generation, value, pairing_permissions(true), wall, now)
                .is_err());
        }
        let gate = PairingGate::default();
        let generation = gate.replace().unwrap();
        assert!(gate
            .install_at(generation, code(), pairing_permissions(false), wall, now)
            .is_err());
    }
    #[test]
    fn concurrent_redemption_authorizes_only_one_local_session() {
        let gate = PairingGate::default();
        let (wall, now) = clocks();
        let generation = gate.replace().unwrap();
        gate.install_at(generation, code(), pairing_permissions(true), wall, now)
            .unwrap();
        let barrier = Arc::new(std::sync::Barrier::new(16));
        let workers: Vec<_> = (0..16)
            .map(|_| {
                let gate = gate.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    gate.consume_at(&request(), now)
                })
            })
            .collect();
        let accepted = workers
            .into_iter()
            .filter_map(|task| (task.join().unwrap() == PairingDecision::Authorized).then_some(()))
            .count();
        assert_eq!(accepted, 1);
    }
    #[test]
    fn view_only_code_cannot_expand_to_input() {
        let gate = PairingGate::default();
        let (wall, now) = clocks();
        let mut value = code();
        value.permissions = pairing_permissions(false);
        let generation = gate.replace().unwrap();
        gate.install_at(generation, value, pairing_permissions(false), wall, now)
            .unwrap();
        assert_eq!(gate.consume_at(&request(), now), PairingDecision::Reject);
    }
    fn activation() -> PairingCode {
        PairingCode {
            mode: Some("activation".into()),
            max_connections: Some(4),
            expires_at: "2027-01-16T08:00:00Z".into(),
            ..code()
        }
    }
    fn activation_request(id: &str, control_input: bool) -> SessionInfo {
        SessionInfo {
            id: id.into(),
            mode: Some("activation".into()),
            permissions: pairing_permissions(control_input),
            ..request()
        }
    }
    #[test]
    fn activation_survives_session_end_but_rejects_replay_and_extra_controllers() {
        let gate = PairingGate::default();
        let (wall, now) = clocks();
        let generation = gate.replace().unwrap();
        gate.install_at(
            generation,
            activation(),
            pairing_permissions(true),
            wall,
            now,
        )
        .unwrap();
        let first = activation_request("first", true);
        assert_eq!(gate.consume_at(&first, now), PairingDecision::Authorized);
        assert!(gate.snapshot_at(now).is_some());
        assert_eq!(gate.consume_at(&first, now), PairingDecision::Reject);
        assert_eq!(
            gate.consume_at(&activation_request("extra-input", true), now),
            PairingDecision::Reject
        );
        for id in ["two", "three", "four"] {
            assert_eq!(
                gate.consume_at(&activation_request(id, false), now),
                PairingDecision::Authorized
            );
        }
        assert_eq!(
            gate.consume_at(&activation_request("five", false), now),
            PairingDecision::Reject
        );
        gate.release(&first);
        assert_eq!(gate.consume_at(&first, now), PairingDecision::Reject);
        assert_eq!(
            gate.consume_at(&activation_request("new-controller", true), now),
            PairingDecision::Authorized
        );
        gate.close();
        assert!(!gate.authorized_now(&activation_request("new-controller", true)));
        assert!(gate.snapshot_at(now).is_none());
    }
    #[test]
    fn activation_mode_generation_expiry_and_replay_ledger_fail_closed() {
        let gate = PairingGate::default();
        let (wall, now) = clocks();
        let first = gate.replace().unwrap();
        gate.install_at(first, activation(), pairing_permissions(true), wall, now)
            .unwrap();
        assert_eq!(gate.consume_at(&request(), now), PairingDecision::Reject);
        assert_eq!(
            gate.consume_at(
                &activation_request("expired", true),
                now + MAX_ACTIVATION_LIFETIME
            ),
            PairingDecision::Reject
        );
        let second = gate.replace().unwrap();
        assert!(!gate
            .install_at(first, activation(), pairing_permissions(true), wall, now)
            .unwrap());
        gate.install_at(second, activation(), pairing_permissions(true), wall, now)
            .unwrap();
        for n in 0..MAX_SESSIONS_PER_ACTIVATION {
            let request = activation_request(&format!("session-{n}"), false);
            assert_eq!(gate.consume_at(&request, now), PairingDecision::Authorized);
            gate.release(&request);
        }
        assert_eq!(
            gate.consume_at(&activation_request("limit", false), now),
            PairingDecision::Reject
        );
        assert!(gate.snapshot_at(now).is_none());
    }
    #[test]
    fn activation_contract_rejects_unknown_mode_or_unbounded_connection_limit() {
        let (wall, _) = clocks();
        for n in [0, 1, 3, 5, usize::MAX] {
            let mut code = activation();
            code.max_connections = Some(n);
            assert!(code.lifetime(pairing_permissions(true), wall).is_err());
        }
        let mut code = activation();
        code.mode = Some("permanent".into());
        assert!(code.lifetime(pairing_permissions(true), wall).is_err());
        let mut code = activation();
        code.expires_at = "2027-01-17T08:00:00Z".into();
        assert!(code.lifetime(pairing_permissions(true), wall).is_err());
    }
}
