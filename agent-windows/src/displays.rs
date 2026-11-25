//! Physical-pixel display topology. Device names stay local to the host; remote
//! display identifiers are scoped to this process and one observed topology epoch.
use anyhow::{ensure, Context, Result};
use rand::RngCore;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex, OnceLock};

pub const MAX_DISPLAYS: usize = 16;
const MAX_EDGE: u32 = 32_768;
const MAX_PIXELS: u64 = 67_108_864;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    fn right(self) -> i64 {
        i64::from(self.left) + i64::from(self.width)
    }
    fn bottom(self) -> i64 {
        i64::from(self.top) + i64::from(self.height)
    }
    fn validate(self, display: bool) -> Result<()> {
        ensure!(self.width > 0 && self.height > 0, "empty display bounds");
        ensure!(
            self.right() <= i64::from(i32::MAX) && self.bottom() <= i64::from(i32::MAX),
            "display bounds overflow"
        );
        if display {
            ensure!(
                self.width <= MAX_EDGE
                    && self.height <= MAX_EDGE
                    && u64::from(self.width) * u64::from(self.height) <= MAX_PIXELS,
                "display exceeds supported capture dimensions"
            );
        }
        Ok(())
    }
    fn contains(self, other: Self) -> bool {
        i64::from(other.left) >= i64::from(self.left)
            && i64::from(other.top) >= i64::from(self.top)
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeDisplay {
    // Internal GDI name is used only for detecting topology changes. Never send
    // this value to a peer or accept one supplied by a peer.
    pub(crate) device: String,
    pub bounds: Rect,
    pub primary: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeLayout {
    pub displays: Vec<NativeDisplay>,
    pub virtual_screen: Rect,
}

impl NativeLayout {
    pub fn checked(mut displays: Vec<NativeDisplay>, virtual_screen: Rect) -> Result<Self> {
        ensure!(
            !displays.is_empty() && displays.len() <= MAX_DISPLAYS,
            "unsupported display count"
        );
        virtual_screen.validate(false)?;
        displays.sort_by(|a, b| a.device.cmp(&b.device));
        ensure!(
            displays.iter().filter(|m| m.primary).count() == 1,
            "display topology needs exactly one primary monitor"
        );
        for (index, display) in displays.iter().enumerate() {
            ensure!(
                !display.device.is_empty()
                    && display.device.len() <= 128
                    && !display.device.contains('\0'),
                "invalid local display identity"
            );
            ensure!(
                index == 0 || displays[index - 1].device != display.device,
                "duplicate display identity"
            );
            display.bounds.validate(true)?;
            ensure!(
                virtual_screen.contains(display.bounds),
                "display outside virtual desktop"
            );
        }
        // The metrics consumed by SendInput must agree with the actual monitor
        // rectangles. Refuse a torn topology snapshot during hotplug changes.
        let left = displays.iter().map(|m| m.bounds.left).min().unwrap();
        let top = displays.iter().map(|m| m.bounds.top).min().unwrap();
        let right = displays.iter().map(|m| m.bounds.right()).max().unwrap();
        let bottom = displays.iter().map(|m| m.bounds.bottom()).max().unwrap();
        ensure!(
            i64::from(left) == i64::from(virtual_screen.left)
                && i64::from(top) == i64::from(virtual_screen.top)
                && right == virtual_screen.right()
                && bottom == virtual_screen.bottom(),
            "display topology changed while reading"
        );
        Ok(Self {
            displays,
            virtual_screen,
        })
    }

    pub fn primary_index(&self) -> usize {
        self.displays.iter().position(|m| m.primary).unwrap()
    }

    pub fn capture_geometry(&self, index: usize) -> Result<crate::input::Geometry> {
        let bounds = self
            .displays
            .get(index)
            .ok_or_else(|| anyhow::anyhow!("unknown display"))?
            .bounds;
        Ok(crate::input::Geometry {
            left: bounds.left,
            top: bounds.top,
            width: bounds.width,
            height: bounds.height,
            monitors: self.displays.len() as u32,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorDescriptor {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayCatalog {
    v: u32,
    #[serde(rename = "type")]
    kind: &'static str,
    selection_mode: &'static str,
    pub selected_monitor_id: String,
    pub monitors: Vec<MonitorDescriptor>,
}

impl DisplayCatalog {
    /// Catalog for the explicit synthetic bars fixture. This returns metadata
    /// only: it cannot be used to create an InputGuard or select a real desktop.
    /// It exercises the same checked layout and catalog encoding as the host.
    #[doc(hidden)]
    pub fn synthetic_test_pattern() -> Result<Self> {
        let bounds = Rect {
            left: 0,
            top: 0,
            width: 640,
            height: 360,
        };
        let layout = NativeLayout::checked(
            vec![NativeDisplay {
                device: "OPENREMOTE-SYNTHETIC-TEST-PATTERN".into(),
                bounds,
                primary: true,
            }],
            bounds,
        )?;
        Ok(DisplaySelection::from_layout(layout, &[0x53; 32], None)?.catalog)
    }
}

/// A session's capture and input share one immutable physical topology. IDs are
/// invalidated by a process restart or any topology change, including primary
/// reassignment. A stale requested ID never falls back to the primary display.
#[derive(Clone, Debug)]
pub struct DisplaySelection {
    pub layout: Arc<NativeLayout>,
    pub index: usize,
    pub catalog: DisplayCatalog,
    topology_nonce: [u8; 32],
}

impl DisplaySelection {
    pub fn current(requested: Option<&str>) -> Result<Self> {
        let (layout, nonce) = observe_current()?;
        Self::from_layout(layout, &nonce, requested)
    }

    fn from_layout(
        layout: NativeLayout,
        nonce: &[u8; 32],
        requested: Option<&str>,
    ) -> Result<Self> {
        if let Some(id) = requested {
            ensure!(valid_monitor_id(id), "invalid monitor selection");
        }
        let mut topology = Sha256::new();
        topology.update(b"OpenRemote display catalog v1\0");
        topology.update(nonce);
        for monitor in &layout.displays {
            topology.update((monitor.device.len() as u32).to_le_bytes());
            topology.update(monitor.device.as_bytes());
            topology.update(monitor.bounds.left.to_le_bytes());
            topology.update(monitor.bounds.top.to_le_bytes());
            topology.update(monitor.bounds.width.to_le_bytes());
            topology.update(monitor.bounds.height.to_le_bytes());
            topology.update([u8::from(monitor.primary)]);
        }
        let topology = topology.finalize();
        let monitors: Vec<_> = layout
            .displays
            .iter()
            .enumerate()
            .map(|(index, monitor)| {
                let mut id = Sha256::new();
                id.update(b"OpenRemote monitor v1\0");
                id.update(topology);
                id.update((index as u32).to_le_bytes());
                MonitorDescriptor {
                    id: format!("{:x}", id.finalize()),
                    name: format!("Display {}", index + 1),
                    width: monitor.bounds.width,
                    height: monitor.bounds.height,
                    primary: monitor.primary,
                }
            })
            .collect();
        let index = match requested {
            Some(id) => monitors.iter().position(|m| m.id == id).context(
                "selected monitor is unavailable; reconnect to refresh the display list",
            )?,
            None => layout.primary_index(),
        };
        let catalog = DisplayCatalog {
            v: 1,
            kind: "display_catalog",
            selection_mode: "reconnect",
            selected_monitor_id: monitors[index].id.clone(),
            monitors,
        };
        ensure!(
            serde_json::to_vec(&catalog)?.len() <= 16 * 1024,
            "display catalog exceeds frame bound"
        );
        Ok(Self {
            layout: Arc::new(layout),
            index,
            catalog,
            topology_nonce: *nonce,
        })
    }

    pub fn verify_current(&self) -> Result<()> {
        let (layout, nonce) = observe_current()?;
        ensure!(
            *self.layout == layout && self.topology_nonce == nonce,
            "display topology changed; the session must reconnect"
        );
        Ok(())
    }

    pub fn description(&self) -> String {
        let monitor = &self.catalog.monitors[self.index];
        format!(
            "{} — {} × {}{}",
            monitor.name,
            monitor.width,
            monitor.height,
            if monitor.primary { " (primary)" } else { "" }
        )
    }
}

pub fn valid_monitor_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

struct TopologyEpoch {
    process_nonce: [u8; 32],
    generation: u64,
    layout: Option<NativeLayout>,
}
impl TopologyEpoch {
    fn observe(&mut self, layout: &NativeLayout) -> Result<[u8; 32]> {
        if self.layout.as_ref() != Some(layout) {
            self.invalidate()?;
            self.layout = Some(layout.clone());
        }
        let mut hash = Sha256::new();
        hash.update(b"OpenRemote observed topology epoch v1\0");
        hash.update(self.process_nonce);
        hash.update(self.generation.to_le_bytes());
        Ok(hash.finalize().into())
    }
    fn invalidate(&mut self) -> Result<()> {
        self.generation = self
            .generation
            .checked_add(1)
            .context("display topology epoch exhausted")?;
        self.layout = None;
        Ok(())
    }
}

fn observe_current() -> Result<(NativeLayout, [u8; 32])> {
    static CATALOG: OnceLock<Mutex<Option<TopologyEpoch>>> = OnceLock::new();
    // The native read and epoch update run under one lock so concurrent input
    // and watchdog observations cannot apply topology snapshots out of order.
    let mut guard = CATALOG
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| anyhow::anyhow!("display topology lock failed"))?;
    if guard.is_none() {
        let mut nonce = [0; 32];
        rand::rngs::OsRng
            .try_fill_bytes(&mut nonce)
            .context("secure display-ID random source unavailable")?;
        *guard = Some(TopologyEpoch {
            process_nonce: nonce,
            generation: 0,
            layout: None,
        });
    }
    let epoch = guard
        .as_mut()
        .context("display topology state unavailable")?;
    match enumerate() {
        Ok(layout) => {
            let nonce = epoch.observe(&layout)?;
            Ok((layout, nonce))
        }
        Err(error) => {
            // An incomplete snapshot during unplug/replug is also an epoch
            // boundary. Returning to the same arrangement must not revive IDs.
            epoch.invalidate()?;
            Err(error)
        }
    }
}

/// Map a normalized point in the captured physical monitor to Windows' 16-bit
/// absolute virtual-desktop coordinates. No current-primary lookup is permitted.
pub fn absolute_pointer(display: Rect, virtual_screen: Rect, x: f32, y: f32) -> Result<(i32, i32)> {
    display.validate(true)?;
    virtual_screen.validate(false)?;
    ensure!(
        virtual_screen.contains(display),
        "display outside virtual desktop"
    );
    ensure!(
        x.is_finite() && y.is_finite() && (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y),
        "pointer outside display"
    );
    let map = |position: f32, start: i32, size: u32, virtual_start: i32, virtual_size: u32| {
        let physical = f64::from(start) + f64::from(position) * f64::from(size.saturating_sub(1));
        (((physical - f64::from(virtual_start)) * 65_535.0
            / f64::from(virtual_size.saturating_sub(1).max(1)))
        .round() as i32)
            .clamp(0, 65_535)
    };
    Ok((
        map(
            x,
            display.left,
            display.width,
            virtual_screen.left,
            virtual_screen.width,
        ),
        map(
            y,
            display.top,
            display.height,
            virtual_screen.top,
            virtual_screen.height,
        ),
    ))
}

#[cfg(windows)]
pub fn initialize_dpi_awareness() -> Result<()> {
    use windows_sys::Win32::UI::HiDpi::*;
    unsafe {
        let changed = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        ensure!(
            changed != 0
                || AreDpiAwarenessContextsEqual(
                    GetThreadDpiAwarenessContext(),
                    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2
                ) != 0,
            "per-monitor physical-pixel DPI awareness could not be established"
        );
    }
    Ok(())
}

#[cfg(windows)]
pub fn enumerate() -> Result<NativeLayout> {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::{
        Foundation::{BOOL, LPARAM, RECT},
        Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW},
        UI::WindowsAndMessaging::{
            GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
            SM_YVIRTUALSCREEN,
        },
    };
    let _dpi = DpiGuard::enter()?;
    struct Enumeration {
        displays: Vec<NativeDisplay>,
        failed: bool,
    }
    unsafe extern "system" fn visit(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let state = &mut *(data as *mut Enumeration);
        if state.displays.len() >= MAX_DISPLAYS {
            state.failed = true;
            return 0;
        }
        let mut info: MONITORINFOEXW = std::mem::zeroed();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(monitor, &mut info.monitorInfo) == 0 {
            state.failed = true;
            return 0;
        }
        let r = info.monitorInfo.rcMonitor;
        let (Ok(width), Ok(height)) = (
            u32::try_from(i64::from(r.right) - i64::from(r.left)),
            u32::try_from(i64::from(r.bottom) - i64::from(r.top)),
        ) else {
            state.failed = true;
            return 0;
        };
        let length = info
            .szDevice
            .iter()
            .position(|v| *v == 0)
            .unwrap_or(info.szDevice.len());
        let Ok(device) = String::from_utf16(&info.szDevice[..length]) else {
            state.failed = true;
            return 0;
        };
        state.displays.push(NativeDisplay {
            device,
            bounds: Rect {
                left: r.left,
                top: r.top,
                width,
                height,
            },
            primary: info.monitorInfo.dwFlags & 1 != 0, // MONITORINFOF_PRIMARY
        });
        1
    }
    let mut state = Enumeration {
        displays: Vec::new(),
        failed: false,
    };
    unsafe {
        ensure!(
            EnumDisplayMonitors(
                null_mut(),
                null(),
                Some(visit),
                &mut state as *mut _ as LPARAM
            ) != 0
                && !state.failed,
            "could not enumerate physical desktop displays"
        );
        let width = u32::try_from(GetSystemMetrics(SM_CXVIRTUALSCREEN))?;
        let height = u32::try_from(GetSystemMetrics(SM_CYVIRTUALSCREEN))?;
        NativeLayout::checked(
            state.displays,
            Rect {
                left: GetSystemMetrics(SM_XVIRTUALSCREEN),
                top: GetSystemMetrics(SM_YVIRTUALSCREEN),
                width,
                height,
            },
        )
    }
}

#[cfg(not(windows))]
pub fn enumerate() -> Result<NativeLayout> {
    anyhow::bail!("display enumeration requires an interactive Windows desktop")
}

/// Synchronous scope only: the previous context belongs to this exact OS thread.
#[cfg(windows)]
struct DpiGuard(windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT);
#[cfg(windows)]
impl DpiGuard {
    fn enter() -> Result<Self> {
        use windows_sys::Win32::UI::HiDpi::*;
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        ensure!(
            !previous.is_null(),
            "could not enter physical-pixel DPI context"
        );
        Ok(Self(previous))
    }
}
#[cfg(windows)]
impl Drop for DpiGuard {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::UI::HiDpi::SetThreadDpiAwarenessContext(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rect(left: i32, top: i32, width: u32, height: u32) -> Rect {
        Rect {
            left,
            top,
            width,
            height,
        }
    }
    fn display(device: &str, bounds: Rect, primary: bool) -> NativeDisplay {
        NativeDisplay {
            device: device.into(),
            bounds,
            primary,
        }
    }
    #[test]
    fn maps_left_and_above_displays_without_primary_assumptions() {
        let virtual_screen = rect(-1920, -1080, 3840, 2160);
        assert_eq!(
            absolute_pointer(rect(-1920, -1080, 1920, 1080), virtual_screen, 0.0, 0.0).unwrap(),
            (0, 0)
        );
        assert_eq!(
            absolute_pointer(rect(0, 0, 1920, 1080), virtual_screen, 1.0, 1.0).unwrap(),
            (65535, 65535)
        );
        assert_eq!(
            absolute_pointer(
                rect(-1920, 0, 1920, 1080),
                rect(-1920, 0, 3840, 1080),
                1.0,
                1.0
            )
            .unwrap(),
            (32759, 65535)
        );
    }
    #[test]
    fn maps_portrait_and_one_pixel_surfaces_and_rejects_nonfinite() {
        let portrait = rect(-1080, 0, 1080, 1920);
        assert_eq!(
            absolute_pointer(portrait, portrait, 0.0, 1.0).unwrap(),
            (0, 65535)
        );
        let single = rect(0, 0, 1, 1);
        assert_eq!(absolute_pointer(single, single, 0.5, 0.5).unwrap(), (0, 0));
        for x in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            assert!(absolute_pointer(portrait, portrait, x, 0.5).is_err());
        }
    }
    #[test]
    fn topology_is_order_independent_but_detects_movement_identity_and_primary_change() {
        let left = display("left", rect(-1920, 0, 1920, 1080), false);
        let primary = display("primary", rect(0, 0, 1920, 1080), true);
        let v = rect(-1920, 0, 3840, 1080);
        let a = NativeLayout::checked(vec![left.clone(), primary.clone()], v).unwrap();
        let b = NativeLayout::checked(vec![primary.clone(), left.clone()], v).unwrap();
        assert_eq!(a, b);
        let mut changed = left.clone();
        changed.device = "replacement".into();
        assert_ne!(
            a,
            NativeLayout::checked(vec![changed, primary.clone()], v).unwrap()
        );
        let mut other_primary = left;
        other_primary.primary = true;
        let mut prior_primary = primary;
        prior_primary.primary = false;
        assert_ne!(
            a,
            NativeLayout::checked(vec![other_primary, prior_primary], v).unwrap()
        );
        assert_eq!(a.capture_geometry(a.primary_index()).unwrap().left, 0);
        assert!(a.capture_geometry(99).is_err());
    }
    #[test]
    fn rejects_torn_ambiguous_oversized_and_duplicate_topologies() {
        let one = display("one", rect(0, 0, 1920, 1080), true);
        let v = one.bounds;
        assert!(NativeLayout::checked(vec![], v).is_err());
        assert!(NativeLayout::checked(vec![one.clone(), one.clone()], v).is_err());
        assert!(NativeLayout::checked(vec![one.clone()], rect(0, 0, 3840, 1080)).is_err());
        let mut unmarked = one.clone();
        unmarked.primary = false;
        assert!(NativeLayout::checked(vec![unmarked], v).is_err());
        let huge = display("huge", rect(0, 0, 16_384, 16_384), true);
        assert!(NativeLayout::checked(vec![huge.clone()], huge.bounds).is_err());
        let overflow = display("overflow", rect(i32::MAX, 0, 1920, 1080), true);
        assert!(NativeLayout::checked(vec![overflow.clone()], overflow.bounds).is_err());
    }
    #[test]
    fn monitor_ids_bind_to_process_and_exact_topology_and_never_fallback() {
        let one = display("sensitive-local-device-name", rect(0, 0, 1920, 1080), true);
        let second = display("second", rect(-1080, 0, 1080, 1920), false);
        let layout =
            NativeLayout::checked(vec![one.clone(), second], rect(-1080, 0, 3000, 1920)).unwrap();
        let a = DisplaySelection::from_layout(layout.clone(), &[1; 32], None).unwrap();
        let b = DisplaySelection::from_layout(layout.clone(), &[1; 32], None).unwrap();
        assert_eq!(a.catalog.selected_monitor_id, b.catalog.selected_monitor_id);
        let target = a.catalog.monitors.iter().find(|m| !m.primary).unwrap();
        assert!(valid_monitor_id(&target.id));
        let selected =
            DisplaySelection::from_layout(layout.clone(), &[1; 32], Some(&target.id)).unwrap();
        assert_eq!(
            selected
                .layout
                .capture_geometry(selected.index)
                .unwrap()
                .left,
            -1080
        );
        assert!(DisplaySelection::from_layout(layout.clone(), &[2; 32], Some(&target.id)).is_err());
        let single = NativeLayout::checked(vec![one], rect(0, 0, 1920, 1080)).unwrap();
        assert!(DisplaySelection::from_layout(single, &[1; 32], Some(&target.id)).is_err());
        assert!(DisplaySelection::from_layout(layout, &[1; 32], Some(&"0".repeat(64))).is_err());
        let wire = serde_json::to_string(&a.catalog).unwrap();
        assert!(wire.contains("\"selectionMode\":\"reconnect\""));
        assert!(!wire.contains("sensitive-local-device-name"));
        assert!(!wire.contains("left"));
        for bad in ["", "a", &"A".repeat(64), &"g".repeat(64), &"a".repeat(65)] {
            assert!(!valid_monitor_id(bad));
        }
    }
    #[test]
    fn observed_topology_round_trip_and_read_failure_never_revive_old_ids() {
        let a = NativeLayout::checked(
            vec![display("a", rect(0, 0, 1920, 1080), true)],
            rect(0, 0, 1920, 1080),
        )
        .unwrap();
        let b = NativeLayout::checked(
            vec![display("b", rect(0, 0, 1280, 720), true)],
            rect(0, 0, 1280, 720),
        )
        .unwrap();
        let mut epochs = TopologyEpoch {
            process_nonce: [7; 32],
            generation: 0,
            layout: None,
        };
        let initial = epochs.observe(&a).unwrap();
        assert_eq!(initial, epochs.observe(&a).unwrap());
        let first = DisplaySelection::from_layout(a.clone(), &initial, None).unwrap();
        assert_ne!(initial, epochs.observe(&b).unwrap());
        let returned = epochs.observe(&a).unwrap();
        assert_ne!(initial, returned);
        assert!(DisplaySelection::from_layout(
            a.clone(),
            &returned,
            Some(&first.catalog.selected_monitor_id)
        )
        .is_err());
        epochs.invalidate().unwrap();
        assert_ne!(returned, epochs.observe(&a).unwrap());
    }
}
