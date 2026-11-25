//! Bounded owner-visible live connection state. Admission never means connected:
//! only media + an acknowledged coordinator transition can set Connected.
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

pub const MAX_CONNECTIONS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Reconnecting,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    pub name: String,
    pub control_input: bool,
    pub state: ConnectionState,
}

#[derive(Clone, Default)]
pub struct Connections(Arc<Mutex<BTreeMap<String, Connection>>>);

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub devices: Vec<Connection>,
}
impl Snapshot {
    pub fn connected_count(&self) -> usize {
        self.devices
            .iter()
            .filter(|d| d.state == ConnectionState::Connected)
            .count()
    }
    pub fn waiting_count(&self) -> usize {
        self.devices.len() - self.connected_count()
    }
    pub fn summary(&self) -> String {
        format!(
            "{} connected · {} connecting",
            self.connected_count(),
            self.waiting_count()
        )
    }
    pub fn device_list(&self) -> String {
        if self.devices.is_empty() {
            return "No devices connected. Share your Server ID and PIN to invite a device.".into();
        }
        self.devices
            .iter()
            .map(|device| {
                let status = match device.state {
                    ConnectionState::Connecting => "Connecting",
                    ConnectionState::Connected => "Connected",
                    ConnectionState::Reconnecting => "Reconnecting",
                };
                let access = if device.control_input {
                    "control"
                } else {
                    "view only"
                };
                format!("{} — {} ({})", device.name, status, access)
            })
            .collect::<Vec<_>>()
            .join("\r\n")
    }
    pub fn tooltip(&self) -> String {
        let mut text = format!("OpenRemote: {} connected", self.connected_count());
        let connected = self
            .devices
            .iter()
            .filter(|d| d.state == ConnectionState::Connected)
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>();
        if !connected.is_empty() {
            text.push_str(" · ");
            text.push_str(&connected.join(", "));
        }
        if self.waiting_count() != 0 {
            text.push_str(&format!(" · {} connecting", self.waiting_count()));
        }
        truncate_utf16(&text, 127)
    }
}

/// Reject formatting controls and line breaks without corrupting surrogate pairs.
/// Device labels are untrusted, descriptive text rather than verified identities.
pub fn safe_device_name(value: &str) -> String {
    let cleaned: String = value.chars().filter(|c| !c.is_control() && !matches!(*c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{2028}' | '\u{2029}')).collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = truncate_utf16(&cleaned, 64);
    if cleaned.is_empty() {
        "Unnamed device".into()
    } else {
        cleaned
    }
}

pub fn truncate_utf16(value: &str, limit: usize) -> String {
    let mut units = 0;
    value
        .chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= limit
        })
        .collect()
}

pub struct Listing {
    connections: Connections,
    id: String,
}
#[derive(Clone)]
pub struct Monitor {
    connections: Connections,
    id: String,
}
impl Monitor {
    pub fn set_state(&self, state: ConnectionState) {
        if let Ok(mut entries) = self.connections.0.lock() {
            if let Some(entry) = entries.get_mut(&self.id) {
                entry.state = state;
            }
        }
    }
}
impl Listing {
    pub fn monitor(&self) -> Monitor {
        Monitor {
            connections: self.connections.clone(),
            id: self.id.clone(),
        }
    }
}
impl Drop for Listing {
    fn drop(&mut self) {
        if let Ok(mut entries) = self.connections.0.lock() {
            entries.remove(&self.id);
        }
    }
}
impl Connections {
    pub fn reserve(&self, id: &str, name: &str, control_input: bool) -> Option<Listing> {
        if !(1..=128).contains(&id.len())
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return None;
        }
        let mut entries = self.0.lock().ok()?;
        if entries.len() >= MAX_CONNECTIONS
            || entries.contains_key(id)
            || (control_input && entries.values().any(|entry| entry.control_input))
        {
            return None;
        }
        entries.insert(
            id.into(),
            Connection {
                name: safe_device_name(name),
                control_input,
                state: ConnectionState::Connecting,
            },
        );
        Some(Listing {
            connections: self.clone(),
            id: id.into(),
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            devices: self
                .0
                .lock()
                .map(|entries| entries.values().cloned().collect())
                .unwrap_or_default(),
        }
    }
    pub fn clear(&self) {
        if let Ok(mut entries) = self.0.lock() {
            entries.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admission_media_ack_recovery_and_disconnect_have_distinct_counts() {
        let list = Connections::default();
        let first = list.reserve("first", "My phone", true).unwrap();
        let second = list.reserve("second", "Laptop", false).unwrap();
        assert_eq!(list.snapshot().connected_count(), 0);
        assert_eq!(list.snapshot().waiting_count(), 2);
        first.monitor().set_state(ConnectionState::Connected);
        assert_eq!(list.snapshot().summary(), "1 connected · 1 connecting");
        first.monitor().set_state(ConnectionState::Reconnecting);
        assert_eq!(list.snapshot().connected_count(), 0);
        assert!(list
            .snapshot()
            .device_list()
            .contains("Reconnecting (control)"));
        first.monitor().set_state(ConnectionState::Connected);
        second.monitor().set_state(ConnectionState::Connected);
        assert_eq!(list.snapshot().connected_count(), 2);
        drop(first);
        assert_eq!(list.snapshot().connected_count(), 1);
        drop(second);
        assert!(list.snapshot().devices.is_empty());
        assert!(list.reserve("next-session", "Tablet", true).is_some());
    }
    #[test]
    fn pending_slots_duplicates_and_input_are_bounded() {
        let list = Connections::default();
        let first = list.reserve("one", "Phone", true).unwrap();
        assert!(list.reserve("one", "Replay", false).is_none());
        assert!(list.reserve("two", "Other controller", true).is_none());
        let mut pending = vec![first];
        for id in ["two", "three", "four"] {
            pending.push(list.reserve(id, id, false).unwrap());
        }
        assert!(list.reserve("five", "Too many", false).is_none());
        assert_eq!(list.snapshot().waiting_count(), 4);
        drop(pending);
        assert_eq!(list.snapshot().waiting_count(), 0);
    }
    #[test]
    fn tooltip_and_names_are_bounded_unicode_and_plain_text() {
        let name = safe_device_name(" \r\n\u{202e}Phone\0 🖥️  ");
        assert_eq!(name, "Phone 🖥️");
        assert_eq!(safe_device_name("\n\0\u{2066}"), "Unnamed device");
        let list = Connections::default();
        let mut guards = vec![];
        for n in 0..4 {
            let guard = list
                .reserve(&n.to_string(), &"🖥️".repeat(80), false)
                .unwrap();
            guard.monitor().set_state(ConnectionState::Connected);
            guards.push(guard);
        }
        let tip = list.snapshot().tooltip();
        assert!(tip.starts_with("OpenRemote: 4 connected"));
        assert!(tip.encode_utf16().count() <= 127);
        assert!(!tip.contains('\n'));
        list.clear();
        guards[0].monitor().set_state(ConnectionState::Connected);
        assert_eq!(list.snapshot().connected_count(), 0);
    }
}
