use crate::{
    config::{data_dir, Config},
    identity::{atomic_secret_write, protect, unprotect},
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
    pub async fn enroll_or_load(cfg: &Config, public_key: &str) -> Result<Self> {
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
            let token = std::env::var("OPENREMOTE_ENROLLMENT_TOKEN")
                .context("set OPENREMOTE_ENROLLMENT_TOKEN for first enrollment")?;
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
    pub fn display_id(&self) -> &str {
        &self.creds.device.display_id
    }
    fn auth(&self, r: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        r.bearer_auth(&self.creds.device_token)
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
