use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct PermissionSet {
    pub view_screen: bool,
    pub control_input: bool,
    pub clipboard: bool,
    pub file_transfer: bool,
    pub audio: bool,
    pub power: bool,
}
impl PermissionSet {
    pub fn supported(self, allow_input: bool) -> Self {
        self.supported_tools(allow_input, false, false)
    }
    pub fn supported_tools(
        self,
        allow_input: bool,
        allow_clipboard: bool,
        allow_files: bool,
    ) -> Self {
        Self {
            view_screen: self.view_screen,
            control_input: self.view_screen && self.control_input && allow_input,
            clipboard: self.view_screen && self.clipboard && allow_clipboard,
            file_transfer: self.view_screen && self.file_transfer && allow_files,
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlMessage {
    Pointer {
        action: String,
        x: f32,
        y: f32,
        #[serde(default)]
        buttons: u32,
    },
    Scroll {
        dx: f32,
        dy: f32,
    },
    Key {
        code: String,
        down: bool,
        #[serde(default)]
        modifiers: u32,
    },
    Text {
        text: String,
    },
    ReleaseAll,
}

pub fn parse_control(bytes: &[u8]) -> Result<ControlMessage> {
    ensure!(bytes.len() <= 16 * 1024, "control frame exceeds limit");
    // Reject unknown fields without serde flatten/deny_unknown_fields interaction.
    let obj = crate::session_tools::protocol::scalar_object(bytes)?;
    ensure!(
        obj.get("v").and_then(|v| v.as_u64()) == Some(1),
        "unsupported control protocol"
    );
    let kind = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let allowed: &[&str] = match kind {
        "pointer" => &["v", "type", "action", "x", "y", "buttons"],
        "scroll" => &["v", "type", "dx", "dy"],
        "key" => &["v", "type", "code", "down", "modifiers"],
        "text" => &["v", "type", "text"],
        "release_all" => &["v", "type"],
        _ => bail!("unknown control message"),
    };
    ensure!(
        obj.keys().all(|k| allowed.contains(&k.as_str())),
        "unknown control field"
    );
    let m: ControlMessage = serde_json::from_value(serde_json::Value::Object(obj))?;
    match &m {
        ControlMessage::Pointer {
            action,
            x,
            y,
            buttons,
        } => {
            ensure!(
                matches!(action.as_str(), "move" | "down" | "up"),
                "invalid pointer action"
            );
            ensure!(
                x.is_finite()
                    && y.is_finite()
                    && (0.0..=1.0).contains(x)
                    && (0.0..=1.0).contains(y),
                "pointer outside desktop"
            );
            ensure!(*buttons <= 7, "invalid button mask");
        }
        ControlMessage::Scroll { dx, dy } => ensure!(
            dx.is_finite() && dy.is_finite() && dx.abs() <= 2400.0 && dy.abs() <= 2400.0,
            "scroll outside limit"
        ),
        ControlMessage::Key {
            code, modifiers, ..
        } => {
            ensure!(virtual_key(code).is_some(), "unsupported key");
            ensure!(*modifiers <= 15, "invalid modifiers");
        }
        ControlMessage::Text { text } => ensure!(
            !text.is_empty() && text.len() <= 4096 && !text.contains('\0'),
            "invalid text"
        ),
        ControlMessage::ReleaseAll => {}
    }
    Ok(m)
}

pub fn virtual_key(code: &str) -> Option<u16> {
    Some(match code {
        "Enter" => 0x0D,
        "Escape" => 0x1B,
        "Tab" => 0x09,
        "Backspace" => 0x08,
        "Space" => 0x20,
        "Delete" => 0x2E,
        "Insert" => 0x2D,
        "Home" => 0x24,
        "End" => 0x23,
        "PageUp" => 0x21,
        "PageDown" => 0x22,
        "ArrowLeft" => 0x25,
        "ArrowRight" => 0x27,
        "ArrowUp" => 0x26,
        "ArrowDown" => 0x28,
        "ControlLeft" => 0xA2,
        "ControlRight" => 0xA3,
        "AltLeft" => 0xA4,
        "AltRight" => 0xA5,
        "ShiftLeft" => 0xA0,
        "ShiftRight" => 0xA1,
        "MetaLeft" => 0x5B,
        "MetaRight" => 0x5C,
        s if s.len() == 4 && s.starts_with("Key") && s.as_bytes()[3].is_ascii_uppercase() => {
            s.as_bytes()[3] as u16
        }
        s if s.len() == 6 && s.starts_with("Digit") && s.as_bytes()[5].is_ascii_digit() => {
            s.as_bytes()[5] as u16
        }
        s if s.starts_with('F') => {
            let n: u16 = s[1..].parse().ok()?;
            if !(1..=12).contains(&n) {
                return None;
            }
            0x6F + n
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tools_grants_require_request_and_local_enablement_without_other_privileges() {
        let all = PermissionSet {
            view_screen: true,
            control_input: true,
            clipboard: true,
            file_transfer: true,
            audio: true,
            power: true,
        };
        let allowed = all.supported_tools(false, true, true);
        assert!(allowed.view_screen && allowed.clipboard && allowed.file_transfer);
        assert!(!allowed.control_input && !allowed.audio && !allowed.power);
        let no_request = PermissionSet {
            view_screen: true,
            ..Default::default()
        }
        .supported_tools(true, true, true);
        assert!(!no_request.clipboard && !no_request.file_transfer);
        let no_view = PermissionSet {
            view_screen: false,
            ..all
        }
        .supported_tools(true, true, true);
        assert_eq!(no_view, PermissionSet::default());
    }
    #[test]
    fn rejects_unsupported_version_actions_and_privileges() {
        for data in [
            r#"{"v":2,"type":"release_all"}"#,
            r#"{"v":1,"type":"pointer","action":"execute","x":0.2,"y":0.4}"#,
            r#"{"v":1,"type":"power","action":"shutdown"}"#,
            r#"{"v":1,"type":"release_all","code":"oops"}"#,
        ] {
            assert!(parse_control(data.as_bytes()).is_err(), "{data}");
        }
    }
    #[test]
    fn valid_unicode_and_release() {
        assert!(matches!(
            parse_control(r#"{"v":1,"type":"text","text":"తెలుగు ✓"}"#.as_bytes()).unwrap(),
            ControlMessage::Text { .. }
        ));
        assert!(parse_control(br#"{"v":1,"type":"release_all"}"#).is_ok());
    }
    #[test]
    fn rejects_out_of_bounds_and_large_messages() {
        assert!(
            parse_control(br#"{"v":1,"type":"pointer","action":"move","x":1.1,"y":0}"#).is_err()
        );
        assert!(parse_control(&vec![0; 16385]).is_err());
    }
    #[test]
    fn rejects_duplicate_control_keys_before_overwrite() {
        for data in [
            r#"{"v":9,"v":1,"type":"release_all"}"#,
            r#"{"v":1,"type":"text","type":"release_all"}"#,
            r#"{"v":1,"type":"pointer","action":"move","x":2,"x":0.5,"y":0}"#,
            r#"{"v":1,"type":"pointer","action":"move","x":2,"\u0078":0.5,"y":0}"#,
            r#"{"v":1,"type":"release_all"} {}"#,
        ] {
            assert!(parse_control(data.as_bytes()).is_err(), "{data}");
        }
    }
    #[test]
    fn keyboard_whitelist() {
        assert_eq!(virtual_key("KeyA"), Some(65));
        assert_eq!(virtual_key("F12"), Some(123));
        assert_eq!(virtual_key("F13"), None);
        assert_eq!(virtual_key("Key;"), None);
    }
    #[test]
    fn permissions_never_add_capabilities() {
        let p = PermissionSet {
            view_screen: true,
            control_input: true,
            clipboard: true,
            file_transfer: true,
            audio: true,
            power: true,
        }
        .supported(false);
        assert_eq!(
            p,
            PermissionSet {
                view_screen: true,
                ..Default::default()
            }
        );
    }
}
