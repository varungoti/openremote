use crate::{
    config::{data_dir, Config},
    identity::{atomic_secret_write, protect, unprotect, DeviceIdentity},
    pairing::{valid_pin, PairingCode},
    protocol::PermissionSet,
};
use anyhow::{bail, Context, Result};
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrolledDevice {
    pub id: String,
    pub display_id: String,
    pub name: String,
    #[serde(default)]
    pub public_key: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Credentials {
    device: EnrolledDevice,
    device_token: String,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredCredentials {
    api_origin: String,
    public_key: String,
    credentials: Credentials,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostEvent {
    pub seq: i64,
    #[serde(rename = "type")]
    pub event_type: String,
    pub session_id: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Signal {
    pub seq: i64,
    pub from: String,
    pub kind: String,
    pub payload: Value,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub status: String,
    pub permissions: PermissionSet,
    pub controller_name: String,
    pub controller_public_key: String,
    #[serde(default)]
    pub monitor_id: Option<String>,
    #[serde(default)]
    pub pairing_id: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub host_name: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IceConfig {
    pub ice_servers: Vec<IceServer>,
    #[serde(default)]
    pub ice_transport_policy: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct IceServer {
    pub urls: Vec<String>,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub credential: String,
}

#[derive(Clone)]
pub struct Api {
    base: String,
    http: Client,
    creds: Credentials,
}
impl Api {
    /// Managed registration proves possession of this PC's preserved identity.
    /// It never reads an enrollment secret or reassigns an origin binding.
    pub async fn register_or_load(cfg: &Config, identity: &DeviceIdentity) -> Result<Self> {
        let base = cfg.api_base_url.trim_end_matches('/').to_string();
        let http = Client::builder()
            .https_only(!cfg.allow_loopback_http)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let path = data_dir().join("device.dpapi");
        let public_key = identity.public_key_b64();
        if path.exists() {
            use std::io::Read;
            let mut bytes = Vec::new();
            std::fs::File::open(&path)?
                .take(65_537)
                .read_to_end(&mut bytes)?;
            anyhow::ensure!(bytes.len() <= 65_536, "Saved registration is invalid.");
            let stored: StoredCredentials = serde_json::from_slice(&unprotect(&bytes)?)?;
            anyhow::ensure!(
                stored.api_origin == base && stored.public_key == public_key,
                "Saved registration belongs to another service or identity."
            );
            let existing = Self {
                base: base.clone(),
                http: http.clone(),
                creds: stored.credentials,
            };
            match existing.heartbeat().await {
                Ok(()) => return Ok(existing),
                Err(error)
                    if error
                        .downcast_ref::<ApiFailure>()
                        .is_some_and(|e| e.status == 401) => {}
                Err(error) => return Err(error),
            }
            // A same-key proof may recover a lost rotation response. The server
            // refuses this for legacy or revoked hosts. Never delete the key.
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Challenge {
            api_origin: String,
            challenge_id: String,
            challenge: String,
            expires_at: String,
        }
        let challenge: Challenge = pairing_json(http.post(format!("{base}/v1/hosts/register/challenge"))
            .json(&serde_json::json!({"publicKey":public_key,"name":cfg.device_name,"platform":"windows"})).send().await?).await?;
        anyhow::ensure!(
            challenge.api_origin == base,
            "Registration challenge belongs to another service."
        );
        let proof = identity.sign_challenge(
            &base,
            &challenge.challenge_id,
            &challenge.challenge,
            &challenge.expires_at,
            &cfg.device_name,
        )?;
        let creds: Credentials = pairing_json(
            http.post(format!("{base}/v1/hosts/register"))
                .json(&serde_json::json!({"challengeId":challenge.challenge_id,"signature":proof}))
                .send()
                .await?,
        )
        .await?;
        anyhow::ensure!(
            (16..=64).contains(&creds.device.id.len())
                && creds
                    .device
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
                && creds.device.display_id.len() == 9
                && creds.device.display_id.bytes().all(|c| c.is_ascii_digit())
                && creds.device.name == cfg.device_name
                && creds.device.public_key == public_key
                && (32..=128).contains(&creds.device_token.len())
                && creds
                    .device_token
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'),
            "Invalid automatic registration response."
        );
        let stored = StoredCredentials {
            api_origin: base.clone(),
            public_key,
            credentials: creds.clone(),
        };
        atomic_secret_write(&path, &protect(&serde_json::to_vec(&stored)?)?)?;
        Ok(Self { base, http, creds })
    }
    pub async fn enroll_or_load(cfg: &Config, public_key: &str) -> Result<Self> {
        Self::enroll_or_load_with_token(cfg, public_key, None).await
    }
    /// A locally entered enrollment secret stays in memory. It is never placed
    /// on a command line, copied into settings, or exported to child processes.
    pub async fn enroll_or_load_with_token(
        cfg: &Config,
        public_key: &str,
        enrollment_token: Option<&str>,
    ) -> Result<Self> {
        let base = cfg.api_base_url.trim_end_matches('/').to_string();
        let http = Client::builder()
            .https_only(!cfg.allow_loopback_http)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let path = data_dir().join("device.dpapi");
        let creds = if path.exists() {
            let raw = unprotect(&tokio::fs::read(&path).await?)?;
            let stored: StoredCredentials =
                serde_json::from_slice(&raw).context("read encrypted enrollment")?;
            if stored.api_origin != base || stored.public_key != public_key {
                bail!("Enrollment belongs to another origin/identity; re-enroll explicitly, do not reuse a bearer token across servers");
            }
            stored.credentials
        } else {
            let from_environment = std::env::var("OPENREMOTE_ENROLLMENT_TOKEN").ok();
            let token = enrollment_token
                .or(from_environment.as_deref())
                .context("enter the first-enrollment key in OpenRemote Setup")?;
            if token.len() < 32 {
                bail!("enrollment token must be at least 32 characters");
            }
            let r=http.post(format!("{base}/v1/devices/enroll")).bearer_auth(token)
                .json(&serde_json::json!({"name":cfg.device_name,"platform":"windows","publicKey":public_key})).send().await?;
            let credentials: Credentials = checked(r).await?.json().await?;
            let stored = StoredCredentials {
                api_origin: base.clone(),
                public_key: public_key.into(),
                credentials: credentials.clone(),
            };
            atomic_secret_write(&path, &protect(&serde_json::to_vec(&stored)?)?)?;
            credentials
        };
        Ok(Self { base, http, creds })
    }
    fn saved_credentials() -> Result<Option<StoredCredentials>> {
        let path = data_dir().join("device.dpapi");
        use std::io::Read;
        let file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let mut bytes = Vec::new();
        file.take(65_537).read_to_end(&mut bytes)?;
        if bytes.len() > 65_536 {
            bail!("stored enrollment is too large");
        }
        let stored: StoredCredentials = serde_json::from_slice(&unprotect(&bytes)?)?;
        Ok(Some(stored))
    }
    /// Only non-secret identity metadata is exposed to the local owner window.
    pub fn enrollment_origin() -> Result<Option<String>> {
        Ok(Self::saved_credentials()?.map(|stored| stored.api_origin))
    }
    pub fn saved_pc_id() -> Result<Option<String>> {
        let Some(stored) = Self::saved_credentials()? else {
            return Ok(None);
        };
        let id = stored.credentials.device.display_id;
        anyhow::ensure!(
            id.len() == 9 && id.bytes().all(|b| b.is_ascii_digit()),
            "Invalid saved PC ID."
        );
        Ok(Some(id))
    }
    pub fn display_id(&self) -> &str {
        &self.creds.device.display_id
    }
    fn auth(&self, r: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        r.bearer_auth(&self.creds.device_token)
    }
    pub async fn generate_pairing(
        &self,
        pin: &str,
        permissions: PermissionSet,
    ) -> Result<PairingCode> {
        anyhow::ensure!(valid_pin(pin), "Choose a PIN containing 6 to 12 digits.");
        let response = self
            .auth(self.http.post(format!(
                "{}/v1/hosts/{}/pairing",
                self.base, self.creds.device.id
            )))
            .json(&serde_json::json!({"pin": pin, "permissions": permissions}))
            .send()
            .await?;
        pairing_json(response).await
    }
    pub async fn activate(&self, pin: &str, permissions: PermissionSet) -> Result<PairingCode> {
        anyhow::ensure!(
            crate::pairing::valid_simple_pin(pin),
            "Choose a six-digit PIN."
        );
        let response = self
            .auth(self.http.post(format!(
                "{}/v1/hosts/{}/pairing",
                self.base, self.creds.device.id
            )))
            .json(
                &serde_json::json!({"pin": pin, "permissions": permissions, "mode": "activation"}),
            )
            .send()
            .await?;
        let code: PairingCode = pairing_json(response).await?;
        anyhow::ensure!(
            code.is_activation(),
            "The service does not support active hosting."
        );
        Ok(code)
    }
    /// Only called before this process owns any local session, while the
    /// per-user instance lock is held. Old host versions may leave retained
    /// sessions after a crash. A single explicit host_busy response permits a
    /// bounded authenticated cleanup, never token rotation or identity reset.
    pub async fn activate_with_orphan_cleanup(
        &self,
        pin: &str,
        permissions: PermissionSet,
        stop: &tokio_util::sync::CancellationToken,
    ) -> Result<PairingCode> {
        activation_retry_after_cleanup(
            stop,
            || self.activate(pin, permissions),
            || async {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Reply {
                    sessions: Vec<SessionInfo>,
                }
                let response = self
                    .auth(self.http.get(format!(
                        "{}/v1/hosts/{}/sessions",
                        self.base, self.creds.device.id
                    )))
                    .send()
                    .await?;
                let reply: Reply = bounded_json(response, 65_536).await?;
                anyhow::ensure!(
                    reply.sessions.len() <= 16,
                    "Too many retained sessions to safely recover."
                );
                for session in reply.sessions {
                    valid_id(&session.id)?;
                    anyhow::ensure!(
                        matches!(
                            session.status.as_str(),
                            "pending" | "approved" | "connecting" | "active"
                        ),
                        "Invalid retained session state."
                    );
                    // /end is an idempotent owner action for both pending and live
                    // sessions. Every end response must be acknowledged before retry.
                    self.action(&session.id, "end").await?;
                }
                Ok(())
            },
        )
        .await
    }
    pub async fn current_pairing(&self) -> Result<Option<PairingCode>> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Reply {
            pairing: Option<PairingCode>,
        }
        let response = self
            .auth(self.http.get(format!(
                "{}/v1/hosts/{}/pairing",
                self.base, self.creds.device.id
            )))
            .send()
            .await?;
        Ok(pairing_json::<Reply>(response).await?.pairing)
    }
    pub async fn revoke_pairing(&self) -> Result<()> {
        checked(
            self.auth(self.http.delete(format!(
                "{}/v1/hosts/{}/pairing",
                self.base, self.creds.device.id
            )))
            .send()
            .await?,
        )
        .await?;
        Ok(())
    }
    pub async fn heartbeat(&self) -> Result<()> {
        checked(
            self.auth(self.http.post(format!(
                "{}/v1/hosts/{}/heartbeat",
                self.base, self.creds.device.id
            )))
            .send()
            .await?,
        )
        .await?;
        Ok(())
    }
    pub async fn events(&self, after: i64) -> Result<Vec<HostEvent>> {
        let r = checked(
            self.auth(self.http.get(format!(
                "{}/v1/hosts/{}/events?after={after}",
                self.base, self.creds.device.id
            )))
            .send()
            .await?,
        )
        .await?;
        #[derive(Deserialize)]
        struct R {
            #[serde(default)]
            events: Option<Vec<HostEvent>>,
        }
        Ok(r.json::<R>().await?.events.unwrap_or_default())
    }
    pub async fn session(&self, id: &str) -> Result<SessionInfo> {
        valid_id(id)?;
        #[derive(Deserialize)]
        struct R {
            session: SessionInfo,
        }
        Ok(checked(
            self.auth(self.http.get(format!("{}/v1/sessions/{id}", self.base)))
                .send()
                .await?,
        )
        .await?
        .json::<R>()
        .await?
        .session)
    }
    pub async fn approve(&self, id: &str, permissions: PermissionSet) -> Result<()> {
        valid_id(id)?;
        checked(
            self.auth(
                self.http
                    .post(format!("{}/v1/sessions/{id}/approve", self.base)),
            )
            .json(&serde_json::json!({"permissions":permissions}))
            .send()
            .await?,
        )
        .await?;
        Ok(())
    }
    pub async fn action(&self, id: &str, action: &str) -> Result<()> {
        valid_id(id)?;
        if !matches!(action, "reject" | "end" | "connected") {
            bail!("invalid action");
        }
        checked(
            self.auth(
                self.http
                    .post(format!("{}/v1/sessions/{id}/{action}", self.base)),
            )
            .send()
            .await?,
        )
        .await?;
        Ok(())
    }
    pub async fn ice(&self, id: &str) -> Result<IceConfig> {
        valid_id(id)?;
        Ok(checked(
            self.auth(self.http.get(format!("{}/v1/sessions/{id}/ice", self.base)))
                .send()
                .await?,
        )
        .await?
        .json()
        .await?)
    }
    pub async fn send_signal(&self, id: &str, kind: &str, payload: Value) -> Result<()> {
        valid_id(id)?;
        let body = serde_json::json!({"kind":kind,"payload":payload});
        checked(
            self.auth(
                self.http
                    .post(format!("{}/v1/sessions/{id}/signal", self.base)),
            )
            .json(&body)
            .send()
            .await?,
        )
        .await?;
        Ok(())
    }
    pub async fn signals(&self, id: &str, after: i64) -> Result<Vec<Signal>> {
        valid_id(id)?;
        #[derive(Deserialize)]
        struct R {
            #[serde(default)]
            signals: Option<Vec<Signal>>,
        }
        Ok(checked(
            self.auth(self.http.get(format!(
                "{}/v1/sessions/{id}/signals?after={after}",
                self.base
            )))
            .send()
            .await?,
        )
        .await?
        .json::<R>()
        .await?
        .signals
        .unwrap_or_default())
    }
}
#[derive(Debug)]
pub struct ApiFailure {
    pub status: u16,
    pub code: String,
    pub reset_after: Option<i64>,
}
impl std::fmt::Display for ApiFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "control-plane request rejected: HTTP {} {}",
            self.status, self.code
        )
    }
}
impl std::error::Error for ApiFailure {}
async fn checked(mut r: Response) -> Result<Response> {
    if !r.status().is_success() {
        let status = r.status().as_u16();
        let mut body = Vec::new();
        while let Some(bytes) = r.chunk().await? {
            if body.len() + bytes.len() > 8192 {
                break;
            }
            body.extend_from_slice(&bytes);
        }
        let value: Value = serde_json::from_slice(&body).unwrap_or_default();
        let code: String = value
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("request_failed")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .take(80)
            .collect();
        return Err(ApiFailure {
            status,
            code,
            reset_after: value
                .get("resetAfter")
                .and_then(|v| v.as_i64())
                .filter(|n| *n >= 0),
        }
        .into());
    }
    Ok(r)
}

async fn pairing_json<T: serde::de::DeserializeOwned>(response: Response) -> Result<T> {
    bounded_json(response, 4096).await
}
async fn bounded_json<T: serde::de::DeserializeOwned>(
    response: Response,
    limit: usize,
) -> Result<T> {
    let mut response = checked(response).await?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(
            bytes.len().saturating_add(chunk.len()) <= limit,
            "Invalid service response."
        );
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).context("Invalid service response.")
}

async fn activation_retry_after_cleanup<T, A, AF, C, CF>(
    stop: &tokio_util::sync::CancellationToken,
    mut activate: A,
    cleanup: C,
) -> Result<T>
where
    A: FnMut() -> AF,
    AF: std::future::Future<Output = Result<T>>,
    C: FnOnce() -> CF,
    CF: std::future::Future<Output = Result<()>>,
{
    anyhow::ensure!(!stop.is_cancelled(), "Activation canceled.");
    let result = tokio::select! { biased; _=stop.cancelled()=>bail!("Activation canceled."), result=activate()=>result };
    let needs_cleanup = result
        .as_ref()
        .err()
        .and_then(|error| error.downcast_ref::<ApiFailure>())
        .is_some_and(|failure| failure.status == 409 && failure.code == "host_busy");
    if !needs_cleanup {
        return result;
    }
    anyhow::ensure!(!stop.is_cancelled(), "Activation canceled.");
    tokio::select! { biased; _=stop.cancelled()=>bail!("Activation canceled."), result=tokio::time::timeout(std::time::Duration::from_secs(15), cleanup())=>result.context("Previous connections could not be stopped in time.")?? }
    anyhow::ensure!(!stop.is_cancelled(), "Activation canceled.");
    tokio::select! { biased; _=stop.cancelled()=>bail!("Activation canceled."), result=activate()=>result }
}

fn valid_id(s: &str) -> Result<()> {
    if s.is_empty()
        || s.len() > 128
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        bail!("invalid session ID");
    }
    Ok(())
}

#[cfg(test)]
mod activation_tests {
    use super::*;
    use std::{cell::Cell, future::ready};
    use tokio_util::sync::CancellationToken;
    fn busy() -> anyhow::Error {
        ApiFailure {
            status: 409,
            code: "host_busy".into(),
            reset_after: None,
        }
        .into()
    }
    #[tokio::test]
    async fn orphan_cleanup_is_acknowledged_before_one_retry_only() {
        let attempts = Cell::new(0);
        let cleaned = Cell::new(false);
        let stop = CancellationToken::new();
        let result: Result<()> = activation_retry_after_cleanup(
            &stop,
            || {
                attempts.set(attempts.get() + 1);
                if attempts.get() > 1 {
                    assert!(cleaned.get());
                }
                ready(Err(busy()))
            },
            || {
                cleaned.set(true);
                ready(Ok(()))
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(attempts.get(), 2);
        assert!(cleaned.get());
    }
    #[tokio::test]
    async fn canceled_or_unacknowledged_cleanup_never_retries_activation() {
        for cancel in [false, true] {
            let attempts = Cell::new(0);
            let stop = CancellationToken::new();
            let result: Result<()> = activation_retry_after_cleanup(
                &stop,
                || {
                    attempts.set(attempts.get() + 1);
                    ready(Err(busy()))
                },
                || {
                    if cancel {
                        stop.cancel();
                        ready(Ok(()))
                    } else {
                        ready(Err(anyhow::anyhow!("end was not acknowledged")))
                    }
                },
            )
            .await;
            assert!(result.is_err());
            assert_eq!(attempts.get(), 1);
        }
        let attempts = Cell::new(0);
        let stop = CancellationToken::new();
        stop.cancel();
        let result: Result<()> = activation_retry_after_cleanup(
            &stop,
            || {
                attempts.set(attempts.get() + 1);
                ready(Ok(()))
            },
            || ready(Ok(())),
        )
        .await;
        assert!(result.is_err());
        assert_eq!(attempts.get(), 0);
    }
    #[tokio::test]
    async fn cancellation_interrupts_inflight_orphan_cleanup() {
        let stop = CancellationToken::new();
        let cancel = stop.clone();
        let attempts = Cell::new(0);
        let result: Result<()> = activation_retry_after_cleanup(
            &stop,
            || {
                attempts.set(attempts.get() + 1);
                ready(Err(busy()))
            },
            || async move {
                cancel.cancel();
                std::future::pending::<Result<()>>().await
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(attempts.get(), 1);
    }
    #[tokio::test]
    async fn other_failures_never_authorize_orphan_cleanup() {
        let cleaned = Cell::new(false);
        let stop = CancellationToken::new();
        let result: Result<()> = activation_retry_after_cleanup(
            &stop,
            || {
                ready(Err(ApiFailure {
                    status: 403,
                    code: "host_busy".into(),
                    reset_after: None,
                }
                .into()))
            },
            || {
                cleaned.set(true);
                ready(Ok(()))
            },
        )
        .await;
        assert!(result.is_err());
        assert!(!cleaned.get());
    }
}
