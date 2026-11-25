use crate::config::data_dir;
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};

pub struct DeviceIdentity {
    signing: SigningKey,
}
impl DeviceIdentity {
    pub async fn load_or_create() -> Result<Self> {
        let path = data_dir().join("identity.dpapi");
        let signing = if path.exists() {
            let raw = unprotect(&tokio::fs::read(&path).await?)?;
            let bytes: [u8; 32] = raw.as_slice().try_into().context("invalid host key")?;
            SigningKey::from_bytes(&bytes)
        } else {
            let key = SigningKey::generate(&mut OsRng);
            std::fs::create_dir_all(data_dir())?;
            atomic_secret_write(&path, &protect(&key.to_bytes())?)?;
            key
        };
        Ok(Self { signing })
    }
    pub fn public_key_b64(&self) -> String {
        STANDARD.encode(self.signing.verifying_key().as_bytes())
    }
    /// Sign only this service's bounded, identity-bound registration proof.
    /// This is pure Ed25519 over the decoded payload, with no application prehash.
    pub fn sign_challenge(
        &self,
        origin: &str,
        id: &str,
        challenge: &str,
        expires_at: &str,
        name: &str,
    ) -> Result<String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        self.sign_challenge_at(origin, id, challenge, expires_at, name, now)
    }
    fn sign_challenge_at(
        &self,
        origin: &str,
        id: &str,
        challenge: &str,
        expires_at: &str,
        name: &str,
        now: u64,
    ) -> Result<String> {
        anyhow::ensure!(
            id.len() == 24
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'),
            "Invalid registration challenge."
        );
        anyhow::ensure!(
            challenge.len() <= 2048 && expires_at.len() <= 40 && expires_at.ends_with('Z'),
            "Invalid registration challenge."
        );
        anyhow::ensure!(
            !name.is_empty()
                && name.len() <= 80
                && name.trim() == name
                && !name.chars().any(|c| c.is_control()),
            "Invalid registration identity."
        );
        let bytes = STANDARD
            .decode(challenge)
            .context("Invalid registration challenge.")?;
        anyhow::ensure!(
            bytes.len() <= 1024 && STANDARD.encode(&bytes) == challenge,
            "Invalid registration challenge."
        );
        let text = std::str::from_utf8(&bytes).context("Invalid registration challenge.")?;
        let lines: Vec<_> = text.split('\n').collect();
        anyhow::ensure!(
            lines.len() == 9
                && lines[0] == "OpenRemote.HostRegistration.v1"
                && lines[1] == origin
                && lines[2] == id
                && lines[4] == self.public_key_b64()
                && lines[5] == name
                && lines[6] == "windows"
                && lines[7] == expires_at
                && lines[8].is_empty(),
            "Registration challenge does not match this PC."
        );
        let nonce = STANDARD
            .decode(lines[3])
            .context("Invalid registration challenge.")?;
        anyhow::ensure!(
            nonce.len() == 32 && STANDARD.encode(&nonce) == lines[3],
            "Invalid registration challenge."
        );
        let expiry =
            time::OffsetDateTime::parse(expires_at, &time::format_description::well_known::Rfc3339)
                .context("Invalid registration challenge expiration.")?;
        let expiry = u64::try_from(expiry.unix_timestamp())
            .context("Invalid registration challenge expiration.")?;
        anyhow::ensure!(
            expiry > now && expiry <= now.saturating_add(90),
            "Registration challenge expired or PC clock is incorrect."
        );
        Ok(STANDARD.encode(self.signing.sign(&bytes).to_bytes()))
    }
}

pub fn fingerprint(public_key: &str) -> String {
    Sha256::digest(STANDARD.decode(public_key).unwrap_or_default())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
        .as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Current-user DPAPI deliberately used: this build is an interactive per-user host, never
/// a LocalSystem service. Copying the file to another Windows user does not unlock its token.
pub fn protect(data: &[u8]) -> Result<Vec<u8>> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Cryptography::*;
        let input = CRYPT_INTEGER_BLOB {
            cbData: data.len().try_into()?,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        if CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        ) == 0
        {
            bail!("DPAPI protection failed");
        }
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as _);
        Ok(bytes)
    }
    #[cfg(not(windows))]
    {
        let _ = data;
        bail!("Secret persistence is Windows-only; refusing plaintext fallback")
    }
}
pub fn unprotect(data: &[u8]) -> Result<Vec<u8>> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Cryptography::*;
        let input = CRYPT_INTEGER_BLOB {
            cbData: data.len().try_into()?,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        if CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        ) == 0
        {
            bail!("DPAPI unprotect failed for this Windows user");
        }
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as _);
        Ok(bytes)
    }
    #[cfg(not(windows))]
    {
        let _ = data;
        bail!("Secret persistence is Windows-only; refusing plaintext fallback")
    }
}

pub fn atomic_secret_write(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("new");
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    let mut f = opts
        .open(&tmp)
        .context("create new secret; remove stale .new file only after review")?;
    if let Err(e) = (|| -> Result<()> {
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)?;
        Ok(())
    })() {
        let _ = std::fs::remove_file(tmp);
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registration_signs_exact_domain_separated_payload_for_its_own_key() {
        let identity = DeviceIdentity {
            signing: SigningKey::from_bytes(&[7; 32]),
        };
        let origin = "https://remote.owner.dev";
        let id = "abcdefghijklmnopqrstuvwx";
        let expiry = "2027-01-15T08:01:00Z";
        let nonce = STANDARD.encode([9; 32]);
        let payload = format!("OpenRemote.HostRegistration.v1\n{origin}\n{id}\n{nonce}\n{}\nMy PC\nwindows\n{expiry}\n", identity.public_key_b64());
        let proof = identity
            .sign_challenge_at(
                origin,
                id,
                &STANDARD.encode(&payload),
                expiry,
                "My PC",
                1_800_000_000,
            )
            .unwrap();
        let signature =
            ed25519_dalek::Signature::from_slice(&STANDARD.decode(proof).unwrap()).unwrap();
        identity
            .signing
            .verifying_key()
            .verify_strict(payload.as_bytes(), &signature)
            .unwrap();
        assert!(identity
            .signing
            .verifying_key()
            .verify_strict(b"other operation", &signature)
            .is_err());
    }
    #[test]
    fn registration_refuses_rebound_expired_or_noncanonical_challenges() {
        let identity = DeviceIdentity {
            signing: SigningKey::from_bytes(&[7; 32]),
        };
        let origin = "https://remote.owner.dev";
        let id = "abcdefghijklmnopqrstuvwx";
        let expiry = "2027-01-15T08:01:00Z";
        let payload = format!(
            "OpenRemote.HostRegistration.v1\n{origin}\n{id}\n{}\n{}\nMy PC\nwindows\n{expiry}\n",
            STANDARD.encode([9; 32]),
            identity.public_key_b64()
        );
        for value in [
            payload.replace("HostRegistration.v1", "ControllerLogin.v1"),
            payload.replace(origin, "https://other.owner.dev"),
            payload.replace("My PC", "Another PC"),
            payload.replace("windows", "linux"),
            payload.replace(&identity.public_key_b64(), &STANDARD.encode([8; 32])),
            payload.replace(&STANDARD.encode([9; 32]), &STANDARD.encode([9; 31])),
            format!("{payload}extra"),
        ] {
            assert!(identity
                .sign_challenge_at(
                    origin,
                    id,
                    &STANDARD.encode(value),
                    expiry,
                    "My PC",
                    1_800_000_000
                )
                .is_err());
        }
        let challenge = STANDARD.encode(&payload);
        assert!(identity
            .sign_challenge_at(origin, id, &challenge, expiry, "My PC", 1_800_000_061)
            .is_err());
        assert!(identity
            .sign_challenge_at(origin, id, &challenge, expiry, "My PC", 1_799_999_900)
            .is_err());
        assert!(identity
            .sign_challenge_at(
                origin,
                "different-challenge-1234",
                &challenge,
                expiry,
                "My PC",
                1_800_000_000
            )
            .is_err());
        assert!(identity
            .sign_challenge_at(
                origin,
                id,
                &(challenge + "\n"),
                expiry,
                "My PC",
                1_800_000_000
            )
            .is_err());
    }
    #[test]
    fn fingerprint_is_sha256_not_ad_hoc_hash() {
        assert_eq!(
            fingerprint("YWJj").replace(' ', ""),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
    #[cfg(not(windows))]
    #[test]
    fn no_plaintext_fallback() {
        assert!(protect(b"secret").is_err());
        assert!(unprotect(b"secret").is_err());
    }
}
