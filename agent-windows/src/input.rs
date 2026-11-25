use crate::displays::DisplaySelection;
#[cfg(windows)]
use crate::protocol::virtual_key;
use crate::protocol::{ControlMessage, PermissionSet};
use anyhow::{bail, Result};
use std::collections::HashSet;

/// One guard per remote session. Pressed inputs are released on every teardown path.
#[derive(Default)]
pub struct InputGuard {
    keys: HashSet<u16>,
    unicode_units: HashSet<u16>,
    buttons: u32,
    display: Option<DisplaySelection>,
}
impl InputGuard {
    pub fn for_display(display: DisplaySelection) -> Self {
        Self {
            keys: HashSet::new(),
            unicode_units: HashSet::new(),
            buttons: 0,
            display: Some(display),
        }
    }
    pub fn apply(&mut self, msg: &ControlMessage, permissions: PermissionSet) -> Result<()> {
        if matches!(msg, ControlMessage::ReleaseAll) {
            return self.release_all();
        }
        if !permissions.control_input {
            bail!("control permission denied");
        }
        #[cfg(windows)]
        {
            self.windows_apply(msg)
        }
        #[cfg(not(windows))]
        {
            let _ = (msg, &self.display);
            bail!("native input unavailable off Windows")
        }
    }
    pub fn release_all(&mut self) -> Result<()> {
        #[cfg(windows)]
        {
            self.release_tracked(
                |vk, unicode| send_key(vk, false, unicode),
                |mask| send_button(mask, false),
            )?;
        }
        #[cfg(not(windows))]
        {
            self.keys.clear();
            self.unicode_units.clear();
            self.buttons = 0;
        }
        Ok(())
    }
    #[cfg(any(windows, test))]
    fn release_tracked<K, B>(&mut self, mut release_key: K, mut release_button: B) -> Result<()>
    where
        K: FnMut(u16, bool) -> Result<()>,
        B: FnMut(u32) -> Result<()>,
    {
        let mut first_error = None;
        // Preserve every failed release in the guard. A topology-triggered first
        // attempt may fail temporarily; final teardown must still retry it and
        // latch restart-required if native release cannot be confirmed.
        for vk in self.keys.iter().copied().collect::<Vec<_>>() {
            match release_key(vk, false) {
                Ok(()) => {
                    self.keys.remove(&vk);
                }
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }
        for unit in self.unicode_units.iter().copied().collect::<Vec<_>>() {
            match release_key(unit, true) {
                Ok(()) => {
                    self.unicode_units.remove(&unit);
                }
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }
        for mask in [1, 2, 4] {
            if self.buttons & mask != 0 {
                match release_button(mask) {
                    Ok(()) => self.buttons &= !mask,
                    Err(error) => {
                        first_error.get_or_insert(error);
                    }
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    #[cfg(windows)]
    fn windows_apply(&mut self, msg: &ControlMessage) -> Result<()> {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
        // Never reinterpret a normalized pointer against a newly selected
        // primary screen. Topology changes invalidate this whole session.
        let display = self
            .display
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("input requires a pinned display"))?;
        if let Err(error) = display.verify_current() {
            let _ = self.release_all();
            return Err(error);
        }
        let bounds = display.layout.displays[display.index].bounds;
        let virtual_screen = display.layout.virtual_screen;
        match msg {
            ControlMessage::Pointer {
                action,
                x,
                y,
                buttons,
            } => {
                let (nx, ny) = crate::displays::absolute_pointer(bounds, virtual_screen, *x, *y)?;
                send_mouse(
                    nx,
                    ny,
                    0,
                    MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK | MOUSEEVENTF_MOVE,
                )?;
                if action == "down" {
                    for mask in [1, 2, 4] {
                        if buttons & mask != 0 && self.buttons & mask == 0 {
                            send_button(mask, true)?;
                            self.buttons |= mask;
                        }
                    }
                } else if action == "up" {
                    let released = if *buttons == 0 {
                        self.buttons
                    } else {
                        *buttons
                    };
                    for mask in [1, 2, 4] {
                        if released & mask != 0 && self.buttons & mask != 0 {
                            send_button(mask, false)?;
                            self.buttons &= !mask;
                        }
                    }
                }
            }
            ControlMessage::Scroll { dx, dy } => {
                if *dy != 0.0 {
                    send_mouse(0, 0, *dy as i32 as u32, MOUSEEVENTF_WHEEL)?;
                }
                if *dx != 0.0 {
                    send_mouse(0, 0, *dx as i32 as u32, MOUSEEVENTF_HWHEEL)?;
                }
            }
            ControlMessage::Key {
                code,
                down,
                modifiers,
            } => {
                // modifiers are a mask: Ctrl=1, Alt=2, Shift=4, Meta=8. Make changes tracked.
                for (mask, vk) in [(1, 0xA2u16), (2, 0xA4), (4, 0xA0), (8, 0x5B)] {
                    if modifiers & mask != 0 && !self.keys.contains(&vk) {
                        send_key(vk, true, false)?;
                        self.keys.insert(vk);
                    }
                }
                let vk = virtual_key(code).ok_or_else(|| anyhow::anyhow!("invalid key"))?;
                send_key(vk, *down, false)?;
                if *down {
                    self.keys.insert(vk);
                } else {
                    self.keys.remove(&vk);
                    for (mask, key) in [(1, 0xA2u16), (2, 0xA4), (4, 0xA0), (8, 0x5B)] {
                        if modifiers & mask != 0 && self.keys.contains(&key) {
                            send_key(key, false, false)?;
                            self.keys.remove(&key);
                        }
                    }
                }
            }
            ControlMessage::Text { text } => {
                // UTF-16 code units, including surrogate pairs, delivered with KEYEVENTF_UNICODE.
                for unit in text.encode_utf16() {
                    send_key(unit, true, true)?;
                    self.unicode_units.insert(unit);
                    send_key(unit, false, true)?;
                    self.unicode_units.remove(&unit);
                }
            }
            ControlMessage::ReleaseAll => self.release_all()?,
        }
        Ok(())
    }
}
impl Drop for InputGuard {
    fn drop(&mut self) {
        let _ = self.release_all();
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Geometry {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub monitors: u32,
}
pub fn primary_geometry() -> Result<Geometry> {
    let layout = crate::displays::enumerate()?;
    layout.capture_geometry(layout.primary_index())
}
#[cfg(windows)]
fn send_mouse(dx: i32, dy: i32, data: u32, flags: u32) -> Result<()> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    if unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) } != 1 {
        bail!("Windows blocked mouse input (secure desktop or integrity boundary)");
    }
    Ok(())
}
#[cfg(windows)]
fn send_button(mask: u32, down: bool) -> Result<()> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
    let flags = match (mask, down) {
        (1, true) => MOUSEEVENTF_LEFTDOWN,
        (1, false) => MOUSEEVENTF_LEFTUP,
        (2, true) => MOUSEEVENTF_RIGHTDOWN,
        (2, false) => MOUSEEVENTF_RIGHTUP,
        (4, true) => MOUSEEVENTF_MIDDLEDOWN,
        (4, false) => MOUSEEVENTF_MIDDLEUP,
        _ => bail!("invalid mouse button"),
    };
    send_mouse(0, 0, 0, flags)
}
#[cfg(windows)]
fn send_key(key: u16, down: bool, unicode: bool) -> Result<()> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
    let mut flags = if down { 0 } else { KEYEVENTF_KEYUP };
    if unicode {
        flags |= KEYEVENTF_UNICODE;
    } else if matches!(key, 0x21..=0x28 | 0x2D | 0x2E | 0xA3 | 0xA5 | 0x5B | 0x5C) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: if unicode { 0 } else { key },
                wScan: if unicode { key } else { 0 },
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    if unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) } != 1 {
        bail!("Windows blocked keyboard input (secure desktop or integrity boundary)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readonly_can_release_but_cannot_inject() {
        let mut input = InputGuard::default();
        assert!(input
            .apply(&ControlMessage::ReleaseAll, PermissionSet::default())
            .is_ok());
        assert!(input
            .apply(
                &ControlMessage::Text {
                    text: "owned".into()
                },
                PermissionSet::default()
            )
            .is_err());
        assert!(input
            .apply(
                &ControlMessage::Key {
                    code: "KeyA".into(),
                    down: true,
                    modifiers: 0
                },
                PermissionSet::default()
            )
            .is_err());
    }
    #[test]
    fn failed_releases_stay_tracked_until_a_confirmed_retry() {
        let mut guard = InputGuard::default();
        guard.keys.extend([0xA2, 0x41]);
        guard.unicode_units.insert(0x2713);
        guard.buttons = 3;
        let result = guard.release_tracked(
            |key, unicode| {
                if key == 0xA2 || unicode {
                    bail!("synthetic blocked key release")
                } else {
                    Ok(())
                }
            },
            |mask| {
                if mask == 2 {
                    bail!("synthetic blocked button release")
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.is_err());
        assert_eq!(guard.keys, HashSet::from([0xA2]));
        assert_eq!(guard.unicode_units, HashSet::from([0x2713]));
        assert_eq!(guard.buttons, 2);
        let mut key_retries = Vec::new();
        let mut button_retries = Vec::new();
        guard
            .release_tracked(
                |key, unicode| {
                    key_retries.push((key, unicode));
                    Ok(())
                },
                |mask| {
                    button_retries.push(mask);
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(key_retries, vec![(0xA2, false), (0x2713, true)]);
        assert_eq!(button_retries, vec![2]);
        assert!(guard.keys.is_empty() && guard.unicode_units.is_empty());
        assert_eq!(guard.buttons, 0);
    }
}
