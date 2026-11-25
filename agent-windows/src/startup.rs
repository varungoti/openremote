//! Managed connection preparation and safe diagnostics. The normal app exposes
//! one Activate/Deactivate action and a six-digit PIN; technical helpers below remain
//! internal compatibility utilities and are not a fallback setup interface.
use crate::{config::Config, host_home::phone_setup_link};
use anyhow::{bail, ensure, Context, Result};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

const MAX_CONFIG_BYTES: u64 = 65_536;
pub const SERVICE_PROFILE: &str = include_str!("../../config/service-profile.json");
pub const UNCONFIGURED_MESSAGE: &str = "This installer has no connection service configured. Your PC ID is unavailable. Ask the app provider for a configured installer.";

/// Resolve managed settings without exposing any technical setup fields.
/// A protected enrollment binding wins; otherwise only the provider's build
/// profile may select a service. Existing permissions and trust are preserved.
pub fn managed_config(
    settings: &LoadedSettings,
    bound: Option<&str>,
    profile: &str,
) -> Result<Config> {
    if let Some(raw) = settings.original.as_deref() {
        ensure!(
            serde_json::from_slice::<Config>(raw).is_ok(),
            "Saved app settings could not be restored. Contact the app provider."
        );
    }
    let mut config = settings.config.clone();
    let origin = match bound {
        Some(origin) => {
            phone_setup_link(origin, "000000001").context(
                "Saved app registration could not be restored. Contact the app provider.",
            )?;
            ensure!(!is_placeholder(origin), "{UNCONFIGURED_MESSAGE}");
            origin.to_owned()
        }
        None => service_profile_origin(profile)
            .map_err(|_| anyhow::anyhow!(UNCONFIGURED_MESSAGE))?
            .context(UNCONFIGURED_MESSAGE)?,
    };
    config.api_base_url = origin;
    config.allow_loopback_http = false;
    if settings.original.is_none()
        && (config.device_name.is_empty()
            || config.device_name.len() > 80
            || config.device_name.trim() != config.device_name
            || config.device_name.chars().any(|c| c.is_control()))
    {
        config.device_name = "Windows PC".into();
    }
    Ok(config)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectStage {
    Idle,
    Pin,
    Preparing,
    Hosting,
    Stopping,
    RestartRequired,
}
#[derive(Debug)]
pub struct ConnectState {
    pub stage: ConnectStage,
    pub generation: u64,
    pub closing: bool,
}
impl Default for ConnectState {
    fn default() -> Self {
        Self {
            stage: ConnectStage::Idle,
            generation: 0,
            closing: false,
        }
    }
}
impl ConnectState {
    pub fn label(&self) -> &'static str {
        if matches!(
            self.stage,
            ConnectStage::Preparing | ConnectStage::Hosting | ConnectStage::Stopping
        ) {
            "Deactivate Connection"
        } else {
            "Activate Connection"
        }
    }
    pub fn active(&self) -> bool {
        matches!(
            self.stage,
            ConnectStage::Preparing | ConnectStage::Hosting | ConnectStage::Stopping
        )
    }
    pub fn request_pin(&mut self) -> bool {
        if self.closing || self.stage != ConnectStage::Idle {
            return false;
        }
        self.stage = ConnectStage::Pin;
        true
    }
    pub fn begin(&mut self, pin: &str) -> Result<u64> {
        ensure!(
            !self.closing && self.stage == ConnectStage::Pin,
            "Connection is already running."
        );
        ensure!(
            crate::pairing::valid_simple_pin(pin),
            "Enter a six-digit security PIN."
        );
        self.generation = self
            .generation
            .checked_add(1)
            .context("Restart the app to connect again.")?;
        self.stage = ConnectStage::Preparing;
        Ok(self.generation)
    }
    pub fn ready(&mut self, generation: u64) -> bool {
        if generation != self.generation || self.closing || self.stage != ConnectStage::Preparing {
            return false;
        }
        self.stage = ConnectStage::Hosting;
        true
    }
    pub fn stop(&mut self) {
        if self.active() {
            self.stage = ConnectStage::Stopping;
        }
    }
    pub fn finish(&mut self, generation: u64, restart_required: bool) -> bool {
        if generation != self.generation || !self.active() {
            return false;
        }
        self.stage = if restart_required {
            ConnectStage::RestartRequired
        } else {
            ConnectStage::Idle
        };
        true
    }
}

/// Capability admission is completed before asking the owner to choose a PIN.
pub fn validate_activation_service(value: &serde_json::Value) -> Result<()> {
    ensure!(value.get("service").and_then(|v| v.as_str()) == Some("openremote")
        && value.get("protocolVersion").and_then(|v| v.as_u64()) == Some(1)
        && value.get("pairingCodeVersion").and_then(|v| v.as_u64()) == Some(1)
        && value.get("activationVersion").and_then(|v| v.as_u64()) == Some(1)
        && value.get("maxActivationConnections").and_then(|v| v.as_u64()) == Some(crate::connections::MAX_CONNECTIONS as u64),
        "This service needs an update before this PC can accept connections. Contact the app provider.");
    Ok(())
}

/// This public build profile contains an origin only, never enrollment keys.
pub fn service_profile_origin(raw: &str) -> Result<Option<String>> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Profile {
        schema_version: u32,
        service_name: String,
        api_origin: String,
    }
    ensure!(raw.len() <= 4096, "The app service profile is invalid.");
    let value: Profile =
        serde_json::from_str(raw).context("The app service profile is invalid.")?;
    ensure!(
        value.schema_version == 1 && value.service_name == "OpenRemote",
        "The app service profile is incompatible."
    );
    if value.api_origin.is_empty() {
        return Ok(None);
    }
    phone_setup_link(&value.api_origin, "000000001")
        .context("The app service profile needs a valid HTTPS server origin.")?;
    ensure!(
        !is_placeholder(&value.api_origin),
        "The app service profile contains a sample server address."
    );
    Ok(Some(
        reqwest::Url::parse(&value.api_origin)?
            .origin()
            .ascii_serialization(),
    ))
}

/// Existing saved settings and the DPAPI enrollment always take precedence.
pub fn apply_service_profile(
    settings: &mut LoadedSettings,
    bound: Option<&str>,
    raw: &str,
) -> Result<()> {
    if settings.original.is_some() || bound.is_some() {
        return Ok(());
    }
    if let Some(origin) = service_profile_origin(raw)? {
        settings.config.api_base_url = origin;
        settings.notice = "Your server address is supplied by this app. Select the trusted encoder and enter the enrollment key to register this PC.";
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchMode {
    Run,
}
impl LaunchMode {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let args: Vec<_> = args.into_iter().collect();
        match args.as_slice() {
            [] => Ok(Self::Run),
            [arg] if arg == "--configure" => Ok(Self::Run),
            _ => bail!("Start OpenRemote from its installed application shortcut."),
        }
    }
}

pub struct LoadedSettings {
    pub config: Config,
    pub original: Option<Vec<u8>>,
    pub needs_repair: bool,
    pub notice: &'static str,
}

/// Missing settings are a UI state, not a startup failure. In particular, this
/// function never creates the invalid template written by the old launcher.
pub fn load_settings(path: &Path) -> Result<LoadedSettings> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LoadedSettings {
                config: Config::default(), original: None, needs_repair: true,
                notice: "Welcome. Enter your server details and trusted encoder below. Nothing connects until you choose Start host.",
            });
        }
        Err(error) => return Err(error).context("The settings file could not be opened. Check this Windows account's access to its OpenRemote folder."),
    };
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_CONFIG_BYTES,
        "The settings file is too large. Preserve it and ask the server owner to repair it."
    );
    match serde_json::from_slice::<Config>(&bytes) {
        Ok(config) => {
            let needs_repair = config.validate().is_err() || is_placeholder(&config.api_base_url);
            Ok(LoadedSettings {config, original: Some(bytes), needs_repair,
                notice: if needs_repair { "Setup is incomplete. Correct the fields below, then choose Start host. Existing PC identity and access permissions are preserved." } else { "Ready to check the encoder and connect. Your existing PC identity and access permissions will be preserved." }})
        }
        Err(_) => Ok(LoadedSettings {
            config: Config::default(), original: Some(bytes), needs_repair: true,
            notice: "The settings file is invalid. Safe defaults are shown; unattended access and extra tools are off. Start host will keep a recovery copy before replacing it.",
        }),
    }
}

fn is_placeholder(origin: &str) -> bool {
    reqwest::Url::parse(origin)
        .ok()
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|host| {
            ["example.com", "example.net", "example.org", "invalid"]
                .iter()
                .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
        })
}

pub struct SettingsFields {
    pub server: String,
    pub name: String,
    pub encoder: String,
    pub hash: String,
}

/// Keep permissions/trust intact while changing only the four visible fields.
/// The encrypted enrollment is authoritative for the server binding, even when
/// config.json is missing or malformed.
pub fn apply_fields(
    base: &Config,
    bound_origin: Option<&str>,
    fields: SettingsFields,
) -> Result<Config> {
    let mut cfg = base.clone();
    let server = fields.server.trim();
    let explicit_local_http = base.allow_loopback_http && server.starts_with("http://");
    if explicit_local_http {
        let url =
            reqwest::Url::parse(server).context("Invalid local development server address.")?;
        ensure!(
            matches!(
                url.host_str(),
                Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
            ),
            "Unencrypted HTTP is restricted to previously enabled loopback development mode."
        );
        // Apply the same raw-origin checks without broadening the host set.
        phone_setup_link(&format!("https://{}", &server[7..]), "000000001")?;
    } else {
        phone_setup_link(server, "000000001")
            .context("Enter an HTTPS server origin such as https://remote.your-domain.com, with no path or credentials.")?;
    }
    ensure!(!is_placeholder(server), "Enter your deployed OpenRemote server address. The sample example.com address is not a running service.");
    let canonical = reqwest::Url::parse(server)?.origin().ascii_serialization();
    if let Some(bound) = bound_origin {
        let bound_canonical = reqwest::Url::parse(bound)?.origin().ascii_serialization();
        ensure!(canonical == bound_canonical,
            "This PC is already registered to another server. Keep its registered server address; changing servers requires a deliberate owner re-enrollment.");
        // Retain the exact representation expected by the stored credential.
        cfg.api_base_url = bound.to_string();
    } else {
        cfg.api_base_url = canonical;
    }
    cfg.device_name = fields.name.trim().to_string();
    cfg.ffmpeg_path = PathBuf::from(fields.encoder.trim());
    cfg.ffmpeg_sha256 = fields.hash.trim().to_ascii_lowercase();
    cfg.validate()?;
    ensure!(
        !cfg.ffmpeg_path.as_os_str().is_empty(),
        "Select the trusted FFmpeg executable."
    );
    Ok(cfg)
}

pub fn validate_enrollment_token(token: Option<&str>, enrolled: bool) -> Result<()> {
    if enrolled {
        return Ok(());
    }
    let token =
        token.context("Enter the enrollment key supplied by your OpenRemote server owner.")?;
    ensure!((32..=4096).contains(&token.len()) && token.is_ascii()
        && !token.bytes().any(|b| b.is_ascii_whitespace() || b.is_ascii_control()),
        "The enrollment key must contain 32–4096 ASCII characters without spaces or control characters.");
    Ok(())
}

/// Compare with the snapshot shown to the user, then atomically activate. A
/// valid existing config's permission flags are never regenerated from defaults.
pub fn save_settings(path: &Path, cfg: &Config, original: Option<&[u8]>) -> Result<()> {
    use std::io::Write;
    cfg.validate()?;
    let current = match std::fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(MAX_CONFIG_BYTES + 1).read_to_end(&mut bytes)?;
            ensure!(bytes.len() as u64 <= MAX_CONFIG_BYTES, "The settings changed or exceeded the supported size. Reopen Settings after reviewing the file.");
            Some(bytes)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    ensure!(current.as_deref() == original, "The settings changed while setup was open. Close OpenRemote and reopen Settings to load the new values.");
    let bytes = serde_json::to_vec_pretty(cfg)?;
    ensure!(
        bytes.len() as u64 <= MAX_CONFIG_BYTES,
        "The settings are too large to reopen safely. Shorten the encoder path or review the advanced configuration before saving."
    );
    if current.as_deref() == Some(bytes.as_slice()) {
        return Ok(());
    }
    let parent = path.parent().context("settings parent missing")?;
    std::fs::create_dir_all(parent)?;
    let suffix = format!("{:016x}", rand::random::<u64>());
    let tmp = parent.join(format!("config.setup-{suffix}.new"));
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp)?;
    let result = (|| -> Result<()> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        if let Some(prior) = current.as_deref() {
            if serde_json::from_slice::<Config>(prior).is_err() {
                let backup = parent.join(format!("config.recovery-{suffix}.json"));
                let mut backup_file = std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(backup)?;
                backup_file.write_all(prior)?;
                backup_file.sync_all()?;
            }
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(windows)]
#[path = "startup_windows.rs"]
mod windows;
#[cfg(windows)]
pub use windows::{report_fatal, run, write_diagnostic};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn activation_admission_rejects_legacy_or_wrong_service_before_pin() {
        let valid = serde_json::json!({"service":"openremote","protocolVersion":1,"pairingCodeVersion":1,"activationVersion":1,"maxActivationConnections":4});
        assert!(validate_activation_service(&valid).is_ok());
        for field in [
            "service",
            "protocolVersion",
            "pairingCodeVersion",
            "activationVersion",
            "maxActivationConnections",
        ] {
            let mut invalid = valid.clone();
            invalid.as_object_mut().unwrap().remove(field);
            assert!(validate_activation_service(&invalid).is_err());
        }
        let mut invalid = valid;
        invalid["maxActivationConnections"] = serde_json::json!(8);
        assert!(validate_activation_service(&invalid).is_err());
    }
    #[test]
    fn simple_flow_has_one_primary_action_and_exact_six_digit_pin() {
        let mut state = ConnectState::default();
        assert_eq!(state.label(), "Activate Connection");
        assert!(state.request_pin());
        for invalid in ["12345", "1234567", "１２３４５６", "123 45"] {
            assert!(state.begin(invalid).is_err());
        }
        let generation = state.begin("001234").unwrap();
        assert_eq!(state.label(), "Deactivate Connection");
        assert!(state.begin("123456").is_err());
        assert!(state.ready(generation));
        state.stop();
        assert!(!state.ready(generation));
        assert!(state.finish(generation, false));
        assert_eq!(state.label(), "Activate Connection");
        assert_eq!(state.stage, ConnectStage::Idle);
    }
    #[test]
    fn canceled_and_stale_connection_results_cannot_revive_hosting() {
        let mut state = ConnectState::default();
        state.request_pin();
        let first = state.begin("123456").unwrap();
        state.stop();
        assert!(!state.ready(first));
        assert!(state.finish(first, false));
        state.request_pin();
        let second = state.begin("234567").unwrap();
        assert!(!state.ready(first));
        assert!(!state.finish(first, false));
        assert!(state.ready(second));
        state.closing = true;
        state.stop();
        assert!(!state.ready(second));
        assert!(state.finish(second, true));
        assert_eq!(state.stage, ConnectStage::RestartRequired);
        assert!(!state.request_pin());
    }
    #[test]
    fn managed_setup_never_requests_a_manual_service_and_keeps_existing_trust() {
        let dir = fixture_dir();
        let path = dir.join("config.json");
        let mut settings = load_settings(&path).unwrap();
        let empty = r#"{"schemaVersion":1,"serviceName":"OpenRemote","apiOrigin":""}"#;
        assert_eq!(
            managed_config(&settings, None, empty)
                .unwrap_err()
                .to_string(),
            UNCONFIGURED_MESSAGE
        );
        settings.config.allow_unattended = true;
        settings.config.trusted_controller_keys = vec!["existing-trusted-key".into()];
        let profile =
            r#"{"schemaVersion":1,"serviceName":"OpenRemote","apiOrigin":"https://new.owner.dev"}"#;
        let cfg = managed_config(&settings, Some("https://enrolled.owner.dev"), profile).unwrap();
        assert_eq!(cfg.api_base_url, "https://enrolled.owner.dev");
        assert!(cfg.allow_unattended);
        assert_eq!(
            cfg.trusted_controller_keys,
            settings.config.trusted_controller_keys
        );
        assert_eq!(
            managed_config(&settings, Some("https://enrolled.owner.dev"), empty)
                .unwrap()
                .api_base_url,
            "https://enrolled.owner.dev"
        );
        settings.original = Some(b"{broken".to_vec());
        assert!(managed_config(&settings, Some("https://enrolled.owner.dev"), profile).is_err());
        assert!(!path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
    fn fixture_dir() -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("openremote-startup-{:016x}", rand::random::<u64>()));
        std::fs::create_dir(&p).unwrap();
        p
    }
    fn fields() -> SettingsFields {
        SettingsFields {
            server: "https://remote.owner.example.dev".into(),
            name: "Office PC".into(),
            encoder: "C:\\Tools\\ffmpeg.exe".into(),
            hash: "a".repeat(64),
        }
    }
    #[test]
    fn first_launch_is_an_editable_state_and_does_not_write_a_broken_template() {
        let dir = fixture_dir();
        let path = dir.join("config.json");
        let state = load_settings(&path).unwrap();
        assert!(state.needs_repair);
        assert!(state.original.is_none());
        assert!(!path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn old_sample_settings_route_to_setup() {
        let dir = fixture_dir();
        let path = dir.join("config.json");
        std::fs::write(&path, serde_json::to_vec(&Config::default()).unwrap()).unwrap();
        let state = load_settings(&path).unwrap();
        assert!(state.needs_repair);
        assert!(apply_fields(
            &state.config,
            None,
            SettingsFields {
                server: "https://api.example.com".into(),
                ..fields()
            }
        )
        .is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn malformed_settings_survive_cancel_and_have_a_recovery_copy_on_save() {
        let dir = fixture_dir();
        let path = dir.join("config.json");
        let old = b"{invalid settings";
        std::fs::write(&path, old).unwrap();
        let state = load_settings(&path).unwrap();
        assert!(state.needs_repair);
        assert_eq!(std::fs::read(&path).unwrap(), old);
        let cfg = apply_fields(&state.config, None, fields()).unwrap();
        save_settings(&path, &cfg, state.original.as_deref()).unwrap();
        let copies: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|x| x.unwrap().path())
            .filter(|x| {
                x.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("config.recovery-")
            })
            .collect();
        assert_eq!(copies.len(), 1);
        assert_eq!(std::fs::read(&copies[0]).unwrap(), old);
        assert!(!load_settings(&path).unwrap().needs_repair);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn correcting_settings_preserves_explicit_security_choices() {
        let cfg = Config {
            allow_unattended: true,
            trusted_controller_keys: vec!["exact-key".into()],
            allow_input: false,
            allow_clipboard: true,
            allow_file_transfer: true,
            ..Config::default()
        };
        let edited = apply_fields(&cfg, None, fields()).unwrap();
        assert!(edited.allow_unattended && edited.allow_clipboard && edited.allow_file_transfer);
        assert!(!edited.allow_input);
        assert_eq!(edited.trusted_controller_keys, cfg.trusted_controller_keys);
        assert_eq!(edited.device_name, "Office PC");
    }
    #[test]
    fn encrypted_enrollment_server_binding_cannot_be_reassigned() {
        assert!(apply_fields(
            &Config::default(),
            Some("https://other.owner.dev"),
            fields()
        )
        .is_err());
        let mut f = fields();
        f.server = "https://REMOTE.OWNER.EXAMPLE.DEV:443/".into();
        let c = apply_fields(
            &Config::default(),
            Some("https://remote.owner.example.dev"),
            f,
        )
        .unwrap();
        assert_eq!(c.api_base_url, "https://remote.owner.example.dev");
    }
    #[test]
    fn settings_do_not_accept_repaired_urls_or_placeholder_servers() {
        for server in [
            "http://remote.owner.dev",
            "https://remote.owner.dev/a/..",
            "https://user:secret@remote.owner.dev",
            "https://remote.owner.dev/?token=sensitive",
            "https://remote.owner.dev\\",
            "https://api.example.com",
        ] {
            let f = SettingsFields {
                server: server.into(),
                ..fields()
            };
            assert!(
                apply_fields(&Config::default(), None, f).is_err(),
                "{server}"
            );
        }
    }
    #[test]
    fn concurrent_settings_edit_is_not_overwritten() {
        let dir = fixture_dir();
        let path = dir.join("config.json");
        let state = load_settings(&path).unwrap();
        std::fs::write(&path, b"owner's newer edit").unwrap();
        let cfg = apply_fields(&state.config, None, fields()).unwrap();
        assert!(save_settings(&path, &cfg, state.original.as_deref()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"owner's newer edit");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn oversized_serialized_settings_never_replace_a_readable_configuration() {
        let dir = fixture_dir();
        let path = dir.join("config.json");
        let valid = apply_fields(&Config::default(), None, fields()).unwrap();
        let mut oversized = valid.clone();
        oversized.ffmpeg_path = PathBuf::from("x".repeat(MAX_CONFIG_BYTES as usize));
        assert!(oversized.validate().is_ok());

        assert!(save_settings(&path, &oversized, None).is_err());
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);

        save_settings(&path, &valid, None).unwrap();
        let original = std::fs::read(&path).unwrap();
        assert!(save_settings(&path, &oversized, Some(&original)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(!load_settings(&path).unwrap().needs_repair);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn existing_explicit_loopback_development_mode_survives_setup() {
        let mut f = fields();
        f.server = "http://127.0.0.1:8080".into();
        assert!(apply_fields(&Config::default(), None, f).is_err());
        let mut f = fields();
        f.server = "http://127.0.0.1:8080".into();
        let prior = Config {
            allow_loopback_http: true,
            ..Config::default()
        };
        assert_eq!(
            apply_fields(&prior, None, f).unwrap().api_base_url,
            "http://127.0.0.1:8080"
        );
        let mut f = fields();
        f.server = "http://remote.owner.dev".into();
        assert!(apply_fields(&prior, None, f).is_err());
    }
    #[test]
    fn enrollment_key_is_local_input_with_bounds_and_not_needed_on_restart() {
        assert!(validate_enrollment_token(None, true).is_ok());
        assert!(validate_enrollment_token(None, false).is_err());
        assert!(validate_enrollment_token(Some(&"a".repeat(32)), false).is_ok());
        for t in [
            "short".into(),
            "a".repeat(4097),
            format!("{}\n", "a".repeat(32)),
        ] {
            assert!(validate_enrollment_token(Some(&t), false).is_err());
        }
    }
    #[test]
    fn double_click_and_settings_shortcut_have_explicit_modes() {
        assert_eq!(
            LaunchMode::parse(Vec::<String>::new()).unwrap(),
            LaunchMode::Run
        );
        assert_eq!(
            LaunchMode::parse(["--configure".into()]).unwrap(),
            LaunchMode::Run
        );
        assert!(LaunchMode::parse(["--unknown".into()]).is_err());
        assert!(LaunchMode::parse(["--configure".into(), "--configure".into()]).is_err());
    }
    #[test]
    fn profile_autoconfigures_only_a_fresh_unenrolled_host() {
        let dir = fixture_dir();
        let path = dir.join("config.json");
        let profile = r#"{"schemaVersion":1,"serviceName":"OpenRemote","apiOrigin":"https://remote.owner.dev"}"#;
        let mut state = load_settings(&path).unwrap();
        apply_service_profile(&mut state, None, profile).unwrap();
        assert_eq!(state.config.api_base_url, "https://remote.owner.dev");
        assert!(state.needs_repair);
        assert!(!path.exists());
        state.config.api_base_url = "https://saved.owner.dev".into();
        state.original = Some(b"saved settings".to_vec());
        apply_service_profile(&mut state, None, profile).unwrap();
        assert_eq!(state.config.api_base_url, "https://saved.owner.dev");
        state.original = None;
        apply_service_profile(&mut state, Some("https://enrolled.owner.dev"), profile).unwrap();
        assert_eq!(state.config.api_base_url, "https://saved.owner.dev");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn profile_is_public_strict_https_schema_without_secrets() {
        assert_eq!(
            service_profile_origin(
                r#"{"schemaVersion":1,"serviceName":"OpenRemote","apiOrigin":""}"#
            )
            .unwrap(),
            None
        );
        for origin in [
            "https://remote.example.com",
            "http://remote.owner.dev",
            "https://owner:secret@remote.owner.dev",
            "https://remote.owner.dev/path",
            "https://remote.owner.dev?x=y",
        ] {
            let raw = serde_json::json!({"schemaVersion":1,"serviceName":"OpenRemote","apiOrigin":origin}).to_string();
            assert!(service_profile_origin(&raw).is_err());
        }
        assert!(service_profile_origin(
            r#"{"schemaVersion":1,"serviceName":"OpenRemote","apiOrigin":"","pin":"123456"}"#
        )
        .is_err());
        assert!(service_profile_origin(
            r#"{"schemaVersion":2,"serviceName":"OpenRemote","apiOrigin":""}"#
        )
        .is_err());
        assert!(service_profile_origin(
            r#"{"schemaVersion":1,"schemaVersion":1,"serviceName":"OpenRemote","apiOrigin":""}"#
        )
        .is_err());
        assert_eq!(service_profile_origin(r#"{"schemaVersion":1,"serviceName":"OpenRemote","apiOrigin":"https://127.0.0.1:8443"}"#).unwrap(), Some("https://127.0.0.1:8443".into()));
    }
}
