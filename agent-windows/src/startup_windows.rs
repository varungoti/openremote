//! Interactive desktop host, with a visible PC identity and native notification icon.
//! This stays in the signed-in desktop session; it is not a Session-0 Windows service.
use super::*;
use crate::{
    api::Api,
    config::config_path,
    connections::Connections,
    host_home::{self, HostOutcome},
    identity::DeviceIdentity,
    media,
    pairing::{pairing_permissions, PairingGate},
};
use std::{
    cell::{Cell, RefCell},
    ptr,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command};
use tokio_util::sync::CancellationToken;
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Controls::{SetScrollInfo, EM_LIMITTEXT},
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{
            EnableWindow, GetFocus, RegisterHotKey, SetFocus, UnregisterHotKey, MOD_ALT,
            MOD_CONTROL, MOD_NOREPEAT, VK_ESCAPE, VK_F12, VK_RETURN,
        },
        Shell::{
            ShellExecuteW, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
            NIM_MODIFY, NOTIFYICONDATAW,
        },
        WindowsAndMessaging::*,
    },
};

const TITLE: usize = 101;
const INTRO: usize = 102;
const PIN_LABEL: usize = 103;
const PIN: usize = 104;
const ID_LABEL: usize = 105;
const PC_ID: usize = 106;
const STATUS: usize = 107;
const PRIMARY: usize = 108;
const CONNECTIONS: usize = 109;
const DEVICES: usize = 110;
const VIEWER: usize = 111;
const TIMER: usize = 1;
const HOTKEY: i32 = 1;
const TRAY_CALLBACK: u32 = WM_APP + 1;
const TRAY_ID: u32 = 1;
const TRAY_OPEN: usize = 201;
const TRAY_STOP: usize = 202;
const TRAY_QUIT: usize = 203;
const CANCELED: &str = "Connection deactivated. This PC is not accepting remote access.";

#[derive(Clone)]
struct Prepared {
    cfg: Config,
    api: Api,
}
enum Event {
    Admitted(std::result::Result<Box<Prepared>, String>),
    Progress {
        generation: u64,
        text: &'static str,
    },
    Ready {
        generation: u64,
    },
    Finished {
        generation: u64,
        result: std::result::Result<HostOutcome, String>,
    },
}
struct Panel {
    state: ConnectState,
    status: String,
    pc_id: String,
    prepared: Option<Prepared>,
    can_admit: bool,
    admission_pending: bool,
    admission: Option<tokio::task::JoinHandle<()>>,
    handle: tokio::runtime::Handle,
    app_stop: CancellationToken,
    stop: Option<CancellationToken>,
    gate: Option<PairingGate>,
    connections: Connections,
    restart_required: Arc<AtomicBool>,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    task: Option<tokio::task::JoinHandle<()>>,
    hotkey: bool,
    initialized: bool,
    tray: bool,
    tray_warning: bool,
    taskbar_created: u32,
    shown_tip: String,
    fonts: Vec<HFONT>,
    dpi: u32,
    scroll: i32,
    last_focus: HWND,
    shown_status: String,
    shown_pc_id: String,
    shown_button: String,
    shown_devices: String,
    shown_count: String,
}
impl Drop for Panel {
    fn drop(&mut self) {
        self.app_stop.cancel();
        if let Some(stop) = &self.stop {
            stop.cancel();
        }
        if let Some(gate) = &self.gate {
            gate.close();
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
        if let Some(task) = self.admission.take() {
            task.abort();
        }
        for font in self.fonts.drain(..) {
            unsafe {
                DeleteObject(font);
            }
        }
    }
}
thread_local! {
    static PANEL: RefCell<Option<Panel>> = const { RefCell::new(None) };
    static LIVE: Cell<HWND> = const { Cell::new(ptr::null_mut()) };
}
struct NativeWindow;
impl Drop for NativeWindow {
    fn drop(&mut self) {
        let hwnd = LIVE.with(Cell::get);
        if !hwnd.is_null() {
            unsafe {
                DestroyWindow(hwnd);
            }
        }
        PANEL.with(|state| drop(state.borrow_mut().take()));
    }
}
pub fn write_diagnostic(code: &str) {
    use std::io::Write;
    // Only fixed operation codes are recorded. No URL, token, server response,
    // clipboard, input, file contents, panic payload or raw error is logged.
    if code.len() > 80
        || !code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return;
    }
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
        return;
    };
    let dir = PathBuf::from(local).join("OpenRemote");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("startup.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 65_536) {
        let _ = std::fs::remove_file(dir.join("startup.previous.log"));
        let _ = std::fs::rename(&path, dir.join("startup.previous.log"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "{now} {code}");
    }
}

pub fn report_fatal(message: &str) {
    write_diagnostic("host_startup_failed");
    unsafe {
        MessageBoxW(
            ptr::null_mut(),
            wide(message).as_ptr(),
            wide("OpenRemote — unable to start").as_ptr(),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
        );
    }
}

pub async fn run(_mode: LaunchMode) -> Result<()> {
    let handle = tokio::runtime::Handle::current();
    let stop = CancellationToken::new();
    host_home::install_panic_stop(stop.clone());
    let signal_stop = stop.clone();
    let signal = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            signal_stop.cancel();
        }
    });
    let result = tokio::task::spawn_blocking(move || run_window(handle, stop))
        .await
        .context("OpenRemote could not keep its window open. Reopen the app.")?;
    signal.abort();
    result
}

fn read_managed() -> std::result::Result<(Config, Option<Vec<u8>>), String> {
    let loaded = load_settings(&config_path()).map_err(|_| {
        "Saved app settings could not be restored. Contact the app provider.".to_string()
    })?;
    let bound = Api::enrollment_origin().map_err(|_| "This PC's saved registration could not be restored. Use the same Windows account, or contact the app provider.".to_string())?;
    let mut cfg = managed_config(&loaded, bound.as_deref(), SERVICE_PROFILE)
        .map_err(|error| error.to_string())?;
    crate::bundled_encoder::configure(&mut cfg).map_err(|_| "This installation is incomplete. Reinstall the configured OpenRemote installer supplied by the app provider.".to_string())?;
    cfg.validate().map_err(|_| {
        "This PC's settings could not be restored. Contact the app provider.".to_string()
    })?;
    Ok((cfg, loaded.original))
}

fn run_window(handle: tokio::runtime::Handle, app_stop: CancellationToken) -> Result<()> {
    let _stop_on_exit = host_home::RevokeOnExit(app_stop.clone());
    let _window = NativeWindow;
    let managed = read_managed();
    let can_admit = managed.is_ok();
    let saved_pc_id = Api::saved_pc_id().ok().flatten().unwrap_or_default();
    let initial_status = match &managed {
        Ok(_) => "Connecting this PC to its service…".into(),
        Err(error) => error.clone(),
    };
    unsafe {
        let instance = GetModuleHandleW(ptr::null());
        ensure!(
            !instance.is_null(),
            "OpenRemote could not start. Reopen the app."
        );
        let taskbar_created = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
        ensure!(
            taskbar_created != 0,
            "Windows could not initialize notification-area recovery. Reopen the app."
        );
        let class = wide("OpenRemoteActivationV1");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
            hIcon: LoadIconW(ptr::null_mut(), IDI_APPLICATION),
            hbrBackground: (COLOR_WINDOW + 1) as _,
            ..std::mem::zeroed()
        };
        ensure!(
            RegisterClassW(&wc) != 0,
            "OpenRemote could not create its window. Reopen the app."
        );
        let (tx, rx) = mpsc::channel();
        PANEL.with(|slot| {
            *slot.borrow_mut() = Some(Panel {
                state: ConnectState::default(),
                status: initial_status,
                pc_id: saved_pc_id,
                prepared: None,
                can_admit,
                admission_pending: false,
                admission: None,
                handle,
                app_stop,
                stop: None,
                gate: None,
                connections: Connections::default(),
                restart_required: Arc::new(AtomicBool::new(false)),
                tx,
                rx,
                task: None,
                hotkey: false,
                initialized: false,
                tray: false,
                tray_warning: false,
                taskbar_created,
                shown_tip: String::new(),
                fonts: vec![],
                dpi: 0,
                scroll: 0,
                last_focus: ptr::null_mut(),
                shown_status: String::new(),
                shown_pc_id: String::new(),
                shown_button: String::new(),
                shown_devices: String::new(),
                shown_count: String::new(),
            })
        });
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class.as_ptr(),
            wide("OpenRemote").as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_VSCROLL,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            620,
            640,
            ptr::null_mut(),
            ptr::null_mut(),
            instance,
            ptr::null(),
        );
        ensure!(
            !hwnd.is_null(),
            "OpenRemote could not create its window. Reopen the app."
        );
        for (id, text) in [
            (TITLE, "OpenRemote"),
            (
                INTRO,
                "Receive connections on this PC, or connect to another PC.",
            ),
            (PIN_LABEL, "Choose a six-digit security PIN"),
            (ID_LABEL, "Your PC ID"),
            (STATUS, ""),
            (CONNECTIONS, ""),
        ] {
            control(hwnd, id, "STATIC", text, WS_CHILD | WS_VISIBLE)?;
        }
        control(
            hwnd,
            PIN,
            "EDIT",
            "",
            WS_CHILD | WS_TABSTOP | ES_PASSWORD as u32 | ES_NUMBER as u32 | ES_AUTOHSCROLL as u32,
        )?;
        SendMessageW(GetDlgItem(hwnd, PIN as i32), EM_LIMITTEXT, 6, 0);
        control(
            hwnd,
            PC_ID,
            "EDIT",
            "",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_READONLY as u32 | ES_CENTER as u32,
        )?;
        control(
            hwnd,
            DEVICES,
            "EDIT",
            "",
            WS_CHILD
                | WS_TABSTOP
                | WS_VSCROLL
                | ES_READONLY as u32
                | ES_MULTILINE as u32
                | ES_AUTOVSCROLL as u32,
        )?;
        control(
            hwnd,
            PRIMARY,
            "BUTTON",
            "Activate Connection",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
        )?;
        control(
            hwnd,
            VIEWER,
            "BUTTON",
            "Connect to a PC",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON as u32,
        )?;
        PANEL.with(|state| state.borrow_mut().as_mut().unwrap().initialized = true);
        let dpi = GetDpiForWindow(hwnd);
        ensure!(
            dpi > 0,
            "OpenRemote could not read this display. Reopen the app."
        );
        let mut rect: RECT = std::mem::zeroed();
        ensure!(
            GetWindowRect(hwnd, &mut rect) != 0,
            "OpenRemote could not position its window."
        );
        rect.right = rect.left + scale(620, dpi);
        rect.bottom = rect.top + scale(640, dpi);
        fit_window(hwnd, &rect)?;
        layout(hwnd, dpi)?;
        ensure!(
            SetTimer(hwnd, TIMER, 200, None) != 0,
            "OpenRemote could not start its window timer."
        );
        render(hwnd);
        ShowWindow(hwnd, SW_SHOW);
        if let Ok(managed) = managed {
            start_admission(managed);
        }
        SetFocus(GetDlgItem(hwnd, PC_ID as i32));
        let mut message: MSG = std::mem::zeroed();
        loop {
            match GetMessageW(&mut message, ptr::null_mut(), 0, 0) {
                -1 => bail!("OpenRemote's window stopped unexpectedly. Reopen the app."),
                0 => break,
                _ => {
                    if message.message == WM_KEYDOWN && message.wParam == VK_ESCAPE as usize {
                        request_stop(hwnd, false);
                        continue;
                    }
                    if message.message == WM_KEYDOWN && message.wParam == VK_RETURN as usize {
                        let pin_entry = PANEL.with(|state| {
                            state
                                .borrow()
                                .as_ref()
                                .is_some_and(|s| s.state.stage == ConnectStage::Pin)
                        });
                        if pin_entry || GetFocus() == GetDlgItem(hwnd, PRIMARY as i32) {
                            primary(hwnd);
                            continue;
                        }
                    }
                    if IsDialogMessageW(hwnd, &message) == 0 {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
            }
        }
    }
    Ok(())
}
unsafe fn control(hwnd: HWND, id: usize, class: &str, text: &str, style: u32) -> Result<()> {
    ensure!(
        !CreateWindowExW(
            if class == "EDIT" { WS_EX_CLIENTEDGE } else { 0 },
            wide(class).as_ptr(),
            wide(text).as_ptr(),
            style,
            0,
            0,
            1,
            1,
            hwnd,
            id as _,
            GetModuleHandleW(ptr::null()),
            ptr::null()
        )
        .is_null(),
        "OpenRemote could not create a required control."
    );
    Ok(())
}
fn scale(value: i32, dpi: u32) -> i32 {
    ((i64::from(value) * i64::from(dpi) + 48) / 96) as i32
}
unsafe fn fit_window(hwnd: HWND, desired: &RECT) -> Result<()> {
    let monitor = MonitorFromRect(desired, MONITOR_DEFAULTTONEAREST);
    let mut info: MONITORINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    ensure!(
        !monitor.is_null() && GetMonitorInfoW(monitor, &mut info) != 0,
        "OpenRemote could not position its window."
    );
    let (x, w) = host_home::fit_axis(
        desired.left,
        desired.right - desired.left,
        info.rcWork.left,
        info.rcWork.right,
    )?;
    let (y, h) = host_home::fit_axis(
        desired.top,
        desired.bottom - desired.top,
        info.rcWork.top,
        info.rcWork.bottom,
    )?;
    ensure!(
        SetWindowPos(
            hwnd,
            ptr::null_mut(),
            x,
            y,
            w,
            h,
            SWP_NOZORDER | SWP_NOACTIVATE
        ) != 0,
        "OpenRemote could not position its window."
    );
    Ok(())
}
unsafe fn layout(hwnd: HWND, dpi: u32) -> Result<()> {
    ensure!(
        (48..=960).contains(&dpi),
        "OpenRemote could not read display scaling."
    );
    let mut rect: RECT = std::mem::zeroed();
    ensure!(
        GetClientRect(hwnd, &mut rect) != 0,
        "OpenRemote could not resize its window."
    );
    let (current, pin) = PANEL.with(|state| {
        state
            .borrow()
            .as_ref()
            .map(|s| (s.dpi, s.state.stage == ConnectStage::Pin))
            .unwrap_or((0, false))
    });
    if current != dpi {
        let mut fonts = vec![];
        for size in [11, 20, 28] {
            let font = CreateFontW(
                -(((size * dpi + 36) / 72) as i32),
                0,
                0,
                0,
                FW_NORMAL as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                DEFAULT_PITCH as u32,
                wide("Segoe UI").as_ptr(),
            );
            if font.is_null() {
                for font in fonts {
                    DeleteObject(font);
                }
                bail!("OpenRemote could not create its text display.");
            }
            fonts.push(font);
        }
        for id in [
            TITLE,
            INTRO,
            PIN_LABEL,
            PIN,
            ID_LABEL,
            PC_ID,
            STATUS,
            PRIMARY,
            CONNECTIONS,
            DEVICES,
            VIEWER,
        ] {
            let font = if id == TITLE {
                fonts[1]
            } else if id == PC_ID {
                fonts[2]
            } else {
                fonts[0]
            };
            SendMessageW(GetDlgItem(hwnd, id as i32), WM_SETFONT, font as usize, 1);
        }
        let prior = PANEL.with(|state| {
            let mut state = state.borrow_mut();
            let state = state.as_mut().unwrap();
            state.dpi = dpi;
            std::mem::replace(&mut state.fonts, fonts)
        });
        for font in prior {
            DeleteObject(font);
        }
    }
    let margin = scale(24, dpi);
    let width = (rect.right - 2 * margin).max(1);
    let content_height = rect.bottom.max(scale(520, dpi));
    let previous_scroll = PANEL.with(|slot| slot.borrow().as_ref().map(|s| s.scroll).unwrap_or(0));
    let scroll = previous_scroll.clamp(0, (content_height - rect.bottom).max(0));
    let info = SCROLLINFO {
        cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
        fMask: SIF_RANGE | SIF_PAGE | SIF_POS | SIF_DISABLENOSCROLL,
        nMin: 0,
        nMax: content_height - 1,
        nPage: rect.bottom.max(1) as u32,
        nPos: scroll,
        nTrackPos: 0,
    };
    SetScrollInfo(hwnd, SB_VERT, &info, 1);
    PANEL.with(|slot| {
        if let Some(s) = slot.borrow_mut().as_mut() {
            s.scroll = scroll;
        }
    });
    let footer = content_height - scale(114, dpi);
    for (id, y, height) in [
        (TITLE, 18, 34),
        (INTRO, 60, 38),
        (ID_LABEL, 108, 24),
        (PC_ID, 136, 52),
        (PIN_LABEL, 204, 24),
        (PIN, 232, 40),
        (CONNECTIONS, 280, 26),
    ] {
        ensure!(
            MoveWindow(
                GetDlgItem(hwnd, id as i32),
                margin,
                scale(y, dpi) - scroll,
                width,
                scale(height, dpi),
                1
            ) != 0,
            "OpenRemote could not resize a control."
        );
    }
    let status_y = scale(if pin { 286 } else { 204 }, dpi).min(footer);
    let status_h = if pin {
        (footer - status_y - scale(8, dpi)).max(1)
    } else {
        scale(70, dpi).min((footer - status_y).max(1))
    };
    MoveWindow(
        GetDlgItem(hwnd, STATUS as i32),
        margin,
        status_y - scroll,
        width,
        status_h,
        1,
    );
    let devices_y = scale(314, dpi).min(footer);
    MoveWindow(
        GetDlgItem(hwnd, DEVICES as i32),
        margin,
        devices_y - scroll,
        width,
        (footer - devices_y - scale(8, dpi)).max(1),
        1,
    );
    MoveWindow(
        GetDlgItem(hwnd, VIEWER as i32),
        margin,
        footer - scroll,
        width,
        scale(40, dpi),
        1,
    );
    MoveWindow(
        GetDlgItem(hwnd, PRIMARY as i32),
        margin,
        footer + scale(50, dpi) - scroll,
        width,
        scale(40, dpi),
        1,
    );
    Ok(())
}

unsafe fn scroll_to(hwnd: HWND, position: i32) {
    PANEL.with(|slot| {
        if let Some(s) = slot.borrow_mut().as_mut() {
            s.scroll = position.max(0);
        }
    });
    if layout(hwnd, GetDpiForWindow(hwnd)).is_err() {
        request_stop(hwnd, true);
    }
    InvalidateRect(hwnd, ptr::null(), 1);
}
unsafe fn reveal_focus(hwnd: HWND) {
    let focus = GetFocus();
    let previous = PANEL.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut()?;
        if focus == s.last_focus {
            return None;
        }
        s.last_focus = focus;
        Some(s.scroll)
    });
    let Some(scroll) = previous else {
        return;
    };
    if focus.is_null() || focus == hwnd || IsChild(hwnd, focus) == 0 {
        return;
    }
    let mut child: RECT = std::mem::zeroed();
    let mut viewport: RECT = std::mem::zeroed();
    if GetWindowRect(focus, &mut child) == 0 || GetClientRect(hwnd, &mut viewport) == 0 {
        return;
    }
    let mut point = POINT {
        x: child.left,
        y: child.top,
    };
    if ScreenToClient(hwnd, &mut point) == 0 {
        return;
    }
    let next = host_home::reveal_axis(
        scroll,
        viewport.bottom,
        point.y + scroll,
        child.bottom - child.top,
    );
    if next != scroll {
        scroll_to(hwnd, next);
    }
}
unsafe fn scroll_command(hwnd: HWND, command: i32) {
    let mut info: SCROLLINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<SCROLLINFO>() as u32;
    info.fMask = SIF_ALL;
    if GetScrollInfo(hwnd, SB_VERT, &mut info) == 0 {
        return;
    }
    let step = scale(32, GetDpiForWindow(hwnd));
    let next = match command {
        SB_LINEUP => info.nPos - step,
        SB_LINEDOWN => info.nPos + step,
        SB_PAGEUP => info.nPos - info.nPage as i32,
        SB_PAGEDOWN => info.nPos + info.nPage as i32,
        SB_TOP => info.nMin,
        SB_BOTTOM => info.nMax,
        SB_THUMBPOSITION | SB_THUMBTRACK => info.nTrackPos,
        _ => return,
    };
    scroll_to(hwnd, next);
}

fn start_admission(managed: (Config, Option<Vec<u8>>)) {
    PANEL.with(|slot| {
        let mut slot = slot.borrow_mut(); let Some(state) = slot.as_mut() else { return; };
        if state.admission_pending || state.state.closing { return; }
        state.admission_pending = true; state.status = "Connecting this PC to its service…".into();
        let stop = state.app_stop.child_token(); let tx = state.tx.clone();
        state.admission = Some(state.handle.spawn(async move {
            let cancel = stop.clone();
            let result = host_home::supervised_attempt(stop, async move {
                tokio::select! { biased; _=cancel.cancelled()=>Err(CANCELED.into()), result=admit(managed)=>result }
            }).await.unwrap_or_else(|_| Err("This PC could not finish registration. Retry the service connection.".into()));
            let _ = tx.send(Event::Admitted(result.map(Box::new)));
        }));
    });
}
async fn admit(
    (cfg, original): (Config, Option<Vec<u8>>),
) -> std::result::Result<Prepared, String> {
    check_server(&cfg).await?;
    let identity = DeviceIdentity::load_or_create().await.map_err(|_| {
        "Windows could not restore this PC's protected identity. Contact the app provider."
            .to_string()
    })?;
    let api = Api::register_or_load(&cfg, &identity)
        .await
        .map_err(|error| registration_error(&error))?;
    let id = api.display_id();
    if id.len() != 9 || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err("The service returned an invalid PC ID. Contact the app provider.".into());
    }
    api.heartbeat()
        .await
        .map_err(|error| registration_error(&error))?;
    // A previous process cannot leave a reusable activation running after restart.
    api.revoke_pairing()
        .await
        .map_err(|error| registration_error(&error))?;
    save_settings(&config_path(), &cfg, original.as_deref()).map_err(|_| {
        "OpenRemote could not save this PC's setup. Reopen the app and try again.".to_string()
    })?;
    Ok(Prepared { cfg, api })
}
unsafe fn primary(hwnd: HWND) {
    let (stage, admitted, retry) = PANEL.with(|state| {
        state
            .borrow()
            .as_ref()
            .map(|s| {
                (
                    s.state.stage,
                    s.prepared.is_some(),
                    s.can_admit && !s.admission_pending,
                )
            })
            .unwrap_or((ConnectStage::Idle, false, false))
    });
    match stage {
        ConnectStage::Idle if !admitted && retry => {
            match read_managed() {
                Ok(managed) => start_admission(managed),
                Err(error) => PANEL.with(|slot| {
                    if let Some(s) = slot.borrow_mut().as_mut() {
                        s.can_admit = false;
                        s.status = error;
                    }
                }),
            };
            render(hwnd);
        }
        ConnectStage::Idle if admitted => {
            PANEL.with(|state| { if let Some(state)=state.borrow_mut().as_mut(){if state.state.request_pin(){state.status="Enter a PIN and choose Activate Connection. Share this PC ID and PIN with devices you want to allow.".into();}} });
            let _ = layout(hwnd, GetDpiForWindow(hwnd));
            render(hwnd);
            SetFocus(GetDlgItem(hwnd, PIN as i32));
        }
        ConnectStage::Pin => start_attempt(hwnd),
        ConnectStage::Preparing | ConnectStage::Hosting => request_stop(hwnd, false),
        _ => {}
    }
}
unsafe fn start_attempt(hwnd: HWND) {
    let edit = GetDlgItem(hwnd, PIN as i32);
    let mut buffer = [0u16; 8];
    let length = GetWindowTextLengthW(edit);
    let copied = GetWindowTextW(edit, buffer.as_mut_ptr(), buffer.len() as i32);
    let pin = if length == copied && copied == 6 {
        String::from_utf16(&buffer[..copied as usize]).ok()
    } else {
        None
    };
    let Some(pin) = pin.filter(|value| crate::pairing::valid_simple_pin(value)) else {
        PANEL.with(|slot| {
            if let Some(s) = slot.borrow_mut().as_mut() {
                s.status = "Enter a six-digit security PIN.".into();
            }
        });
        render(hwnd);
        return;
    };
    if RegisterHotKey(
        hwnd,
        HOTKEY,
        MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
        VK_F12 as u32,
    ) == 0
    {
        PANEL.with(|slot|if let Some(s)=slot.borrow_mut().as_mut(){s.status="Emergency stop could not be registered. Close any other OpenRemote window and try again.".into();});
        render(hwnd);
        return;
    }
    SetWindowTextW(edit, wide("").as_ptr());
    let started=PANEL.with(|slot|{
        let mut slot=slot.borrow_mut();let Some(s)=slot.as_mut() else{return false;};let Some(prepared)=s.prepared.clone()else{return false;};let Ok(generation)=s.state.begin(&pin)else{return false;};
        s.hotkey=true;s.status="Preparing this PC for connections…".into();s.tray_warning=false;
        let stop=s.app_stop.child_token();let gate=PairingGate::default();s.stop=Some(stop.clone());s.gate=Some(gate.clone());
        s.connections=Connections::default();s.restart_required=Arc::new(AtomicBool::new(false));
        let connections=s.connections.clone();let restart=s.restart_required.clone();let tx=s.tx.clone();let progress=tx.clone();let final_gate=gate.clone();
        s.task=Some(s.handle.spawn(async move{
            let failure=restart.clone();
            let result=host_home::supervised_attempt(stop.clone(),async move{attempt(pin,generation,prepared,stop,gate,connections,restart,progress).await}).await;
            let result=match result{Ok(value)=>value,Err(_)=>{failure.store(true,Ordering::SeqCst);Err("Remote access stopped unexpectedly. Restart the app before activating again.".into())}};
            final_gate.close();let _=tx.send(Event::Finished{generation,result});
        }));true
    });
    if !started {
        UnregisterHotKey(hwnd, HOTKEY);
    }
    let _ = layout(hwnd, GetDpiForWindow(hwnd));
    render(hwnd);
}
unsafe fn request_stop(hwnd: HWND, close: bool) {
    let destroy = PANEL.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(s) = slot.as_mut() else {
            return true;
        };
        s.state.closing |= close;
        if let Some(gate) = &s.gate {
            gate.close();
        }
        if let Some(stop) = &s.stop {
            stop.cancel();
        }
        s.connections.clear();
        if s.state.active() {
            s.state.stop();
            s.status = "Deactivating connections…".into();
            false
        } else if close {
            s.app_stop.cancel();
            true
        } else if s.state.stage == ConnectStage::RestartRequired {
            false
        } else {
            s.state.stage = ConnectStage::Idle;
            if s.prepared.is_some() {
                s.status = CANCELED.into();
            }
            false
        }
    });
    SetWindowTextW(GetDlgItem(hwnd, PIN as i32), wide("").as_ptr());
    if destroy {
        DestroyWindow(hwnd);
    } else {
        if !close {
            restore_window(hwnd);
        }
        let _ = layout(hwnd, GetDpiForWindow(hwnd));
        render(hwnd);
    }
}
unsafe fn process_events(hwnd: HWND) {
    let (destroy,unregister,ready,finished)=PANEL.with(|slot|{
        let mut slot=slot.borrow_mut();let Some(s)=slot.as_mut()else{return(true,false,false,false);};
        let mut unregister=false;let mut ready=false;let mut finished=false;
        while let Ok(event)=s.rx.try_recv(){match event{
            Event::Admitted(result) if !s.state.closing=>{s.admission=None;s.admission_pending=false;match result{Ok(prepared)=>{s.pc_id=prepared.api.display_id().into();s.prepared=Some(*prepared);s.status="Your PC ID is ready. Activate Connection to choose a PIN and allow access.".into();},Err(error)=>s.status=error}},
            Event::Progress{generation,text} if generation==s.state.generation && s.state.stage==ConnectStage::Preparing=>s.status=text.into(),
            Event::Ready{generation}=>{if s.state.ready(generation){ready=true;s.status="Connection active. Share your PC ID and PIN. Open the tray icon to see devices or deactivate.".into();}},
            Event::Finished{generation,result}=>{let restart=s.restart_required.load(Ordering::SeqCst);if s.state.finish(generation,restart){s.task=None;s.stop=None;s.gate=None;s.connections.clear();unregister=s.hotkey;s.hotkey=false;finished=true;s.status=if restart{"Remote access stopped. Restart the app before activating again.".into()}else{match result{Ok(HostOutcome::Stopped)=>CANCELED.into(),Ok(HostOutcome::Expired)=>"This activation ended or expired. Activate Connection again to allow access.".into(),Err(error)=>error}};}},
            _=>{}
        }}
        if s.state.stage==ConnectStage::Hosting {
            if let Some(snapshot)=s.gate.as_ref().and_then(PairingGate::snapshot){let minutes=snapshot.remaining_seconds.div_ceil(60);s.status=format!("Connection active for up to {}h {:02}m. Up to four devices; one can control. Ctrl+Alt+F12 stops access.{}",minutes/60,minutes%60,if s.tray_warning{" The tray icon is unavailable; keep this window open."}else{""});}
        }
        (s.state.closing&&!s.state.active(),unregister,ready,finished)
    });
    if unregister {
        UnregisterHotKey(hwnd, HOTKEY);
    }
    if finished {
        remove_tray(hwnd);
        if !destroy {
            restore_window(hwnd);
        }
    }
    if destroy {
        DestroyWindow(hwnd);
        return;
    }
    let app_closed = PANEL.with(|state| {
        state
            .borrow()
            .as_ref()
            .is_none_or(|s| s.app_stop.is_cancelled())
    });
    if app_closed {
        request_stop(hwnd, true);
        return;
    }
    if ready {
        if update_tray(hwnd) {
            ShowWindow(hwnd, SW_HIDE);
        } else {
            restore_window(hwnd);
        }
    } else {
        let tray = PANEL.with(|slot| slot.borrow().as_ref().is_some_and(|s| s.tray));
        if tray {
            update_tray(hwnd);
        }
    }
    render(hwnd);
    reveal_focus(hwnd);
}
unsafe fn render(hwnd: HWND) {
    let update = PANEL.with(|slot| {
        let mut slot = slot.borrow_mut();
        let s = slot.as_mut()?;
        let pin = s.state.stage == ConnectStage::Pin;
        let hosting = s.state.stage == ConnectStage::Hosting;
        let button = if s.admission_pending {
            "Connecting to service…"
        } else if s.prepared.is_none() && s.can_admit {
            "Retry service connection"
        } else {
            s.state.label()
        }
        .to_string();
        let id = if s.pc_id.is_empty() {
            if s.admission_pending {
                "Connecting…"
            } else {
                "Unavailable"
            }
            .to_string()
        } else {
            s.pc_id.clone()
        };
        let snapshot = s.connections.snapshot();
        let count = snapshot.summary();
        let devices = snapshot.device_list();
        let change = |shown: &mut String, value: String| {
            if *shown != value {
                *shown = value.clone();
                Some(value)
            } else {
                None
            }
        };
        Some((
            pin,
            hosting,
            change(&mut s.shown_status, s.status.clone()),
            change(&mut s.shown_pc_id, id),
            change(&mut s.shown_button, button),
            change(&mut s.shown_count, count),
            change(&mut s.shown_devices, devices),
            !s.state.closing
                && !s.admission_pending
                && (s.prepared.is_some() || s.can_admit)
                && !matches!(
                    s.state.stage,
                    ConnectStage::Stopping | ConnectStage::RestartRequired
                ),
            s.prepared.is_some() && !s.state.closing,
        ))
    });
    if let Some((pin, hosting, status, id, button, count, devices, enabled, viewer)) = update {
        for control in [PIN_LABEL, PIN] {
            ShowWindow(
                GetDlgItem(hwnd, control as i32),
                if pin { SW_SHOW } else { SW_HIDE },
            );
        }
        for control in [CONNECTIONS, DEVICES] {
            ShowWindow(
                GetDlgItem(hwnd, control as i32),
                if hosting { SW_SHOW } else { SW_HIDE },
            );
        }
        for (control, text) in [
            (STATUS, status),
            (PC_ID, id),
            (PRIMARY, button),
            (CONNECTIONS, count),
            (DEVICES, devices),
        ] {
            if let Some(text) = text {
                SetWindowTextW(GetDlgItem(hwnd, control as i32), wide(&text).as_ptr());
            }
        }
        EnableWindow(GetDlgItem(hwnd, PRIMARY as i32), i32::from(enabled));
        EnableWindow(GetDlgItem(hwnd, VIEWER as i32), i32::from(viewer));
    }
}

unsafe fn restore_window(hwnd: HWND) {
    ShowWindow(hwnd, SW_RESTORE);
    SetForegroundWindow(hwnd);
    SetFocus(GetDlgItem(hwnd, PC_ID as i32));
    reveal_focus(hwnd);
}
unsafe fn tray_data(hwnd: HWND, tip: &str) -> NOTIFYICONDATAW {
    let mut data: NOTIFYICONDATAW = std::mem::zeroed();
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = TRAY_ID;
    data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    data.uCallbackMessage = TRAY_CALLBACK;
    data.hIcon = LoadIconW(ptr::null_mut(), IDI_APPLICATION);
    for (target, source) in data
        .szTip
        .iter_mut()
        .zip(crate::connections::truncate_utf16(tip, 127).encode_utf16())
    {
        *target = source;
    }
    data
}
unsafe fn update_tray(hwnd: HWND) -> bool {
    let Some((present, tip, changed, active)) = PANEL.with(|slot| {
        slot.borrow().as_ref().map(|s| {
            let tip = if s.state.stage == ConnectStage::Stopping {
                "OpenRemote: deactivating connections".into()
            } else {
                s.connections.snapshot().tooltip()
            };
            let changed = tip != s.shown_tip;
            (s.tray, tip, changed, s.state.active())
        })
    }) else {
        return false;
    };
    if !active {
        return false;
    }
    if present && !changed {
        return true;
    }
    let data = tray_data(hwnd, &tip);
    let mut success = Shell_NotifyIconW(if present { NIM_MODIFY } else { NIM_ADD }, &data) != 0;
    if !success && present {
        success = Shell_NotifyIconW(NIM_ADD, &data) != 0;
    }
    PANEL.with(|slot| {
        if let Some(s) = slot.borrow_mut().as_mut() {
            s.tray = success;
            s.tray_warning = !success;
            if success {
                s.shown_tip = tip;
            }
        }
    });
    if !success {
        restore_window(hwnd);
    }
    success
}
unsafe fn remove_tray(hwnd: HWND) {
    let data = tray_data(hwnd, "");
    Shell_NotifyIconW(NIM_DELETE, &data);
    PANEL.with(|slot| {
        if let Some(s) = slot.borrow_mut().as_mut() {
            s.tray = false;
            s.shown_tip.clear();
        }
    });
}
unsafe fn tray_menu(hwnd: HWND) {
    let menu = CreatePopupMenu();
    if menu.is_null() {
        restore_window(hwnd);
        return;
    }
    AppendMenuW(menu, MF_STRING, TRAY_OPEN, wide("Open OpenRemote").as_ptr());
    AppendMenuW(
        menu,
        MF_STRING,
        TRAY_STOP,
        wide("Deactivate Connection").as_ptr(),
    );
    AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
    AppendMenuW(menu, MF_STRING, TRAY_QUIT, wide("Quit OpenRemote").as_ptr());
    let mut point: POINT = std::mem::zeroed();
    GetCursorPos(&mut point);
    SetForegroundWindow(hwnd);
    let action = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
        point.x,
        point.y,
        0,
        hwnd,
        ptr::null(),
    );
    DestroyMenu(menu);
    PostMessageW(hwnd, WM_NULL, 0, 0);
    match action as usize {
        TRAY_OPEN => restore_window(hwnd),
        TRAY_STOP => request_stop(hwnd, false),
        TRAY_QUIT => request_stop(hwnd, true),
        _ => {}
    }
}
unsafe fn launch_viewer(hwnd: HWND) {
    let origin = PANEL.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|s| s.prepared.as_ref().map(|p| p.cfg.api_base_url.clone()))
    });
    let Some(link) = origin.and_then(|origin| host_home::browser_viewer_link(&origin).ok()) else {
        return;
    };
    let result = ShellExecuteW(
        hwnd,
        wide("open").as_ptr(),
        wide(&link).as_ptr(),
        ptr::null(),
        ptr::null(),
        SW_SHOWNORMAL,
    );
    if result as isize <= 32 {
        MessageBoxW(hwnd,wide("OpenRemote could not open the browser viewer. Check that Windows has a default browser.").as_ptr(),wide("OpenRemote").as_ptr(),MB_OK|MB_ICONWARNING);
    }
}
fn initialized() -> bool {
    PANEL.with(|state| state.borrow().as_ref().is_some_and(|s| s.initialized))
}
unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
    let explorer_restart = PANEL.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|s| msg == s.taskbar_created)
    });
    if explorer_restart {
        PANEL.with(|slot| {
            if let Some(s) = slot.borrow_mut().as_mut() {
                s.tray = false;
                s.shown_tip.clear();
            }
        });
        if !update_tray(hwnd) {
            restore_window(hwnd);
        }
        return 0;
    }
    match msg {
        WM_NCCREATE => {
            LIVE.with(|live| live.set(hwnd));
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_COMMAND if (wparam >> 16) == BN_CLICKED as usize => {
            match wparam & 0xffff {
                PRIMARY => primary(hwnd),
                VIEWER => launch_viewer(hwnd),
                _ => {}
            }
            0
        }
        TRAY_CALLBACK => {
            match lparam as u32 {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => restore_window(hwnd),
                WM_RBUTTONUP | WM_CONTEXTMENU => tray_menu(hwnd),
                _ => {}
            }
            0
        }
        WM_HOTKEY if wparam == HOTKEY as usize => {
            request_stop(hwnd, false);
            0
        }
        WM_TIMER if wparam == TIMER => {
            process_events(hwnd);
            0
        }
        WM_SYSCOMMAND
            if (wparam & 0xfff0) == SC_MINIMIZE as usize
                && PANEL.with(|slot| {
                    slot.borrow()
                        .as_ref()
                        .is_some_and(|s| s.tray && s.state.stage == ConnectStage::Hosting)
                }) =>
        {
            ShowWindow(hwnd, SW_HIDE);
            0
        }
        WM_VSCROLL if initialized() => {
            scroll_command(hwnd, (wparam & 0xffff) as i32);
            0
        }
        WM_MOUSEWHEEL if initialized() => {
            let delta = ((wparam >> 16) as u16) as i16;
            let scroll = PANEL.with(|slot| slot.borrow().as_ref().map(|s| s.scroll).unwrap_or(0));
            scroll_to(
                hwnd,
                scroll - (i32::from(delta) * scale(48, GetDpiForWindow(hwnd)) / 120),
            );
            0
        }
        WM_SIZE if initialized() && wparam != SIZE_MINIMIZED as usize => {
            if layout(hwnd, GetDpiForWindow(hwnd)).is_err() {
                request_stop(hwnd, true);
            }
            0
        }
        WM_DPICHANGED if initialized() => {
            if lparam != 0
                && (fit_window(hwnd, &*(lparam as *const RECT)).is_err()
                    || layout(hwnd, (wparam & 0xffff) as u32).is_err())
            {
                request_stop(hwnd, true);
            }
            0
        }
        WM_CLOSE => {
            let hide = PANEL.with(|slot| {
                slot.borrow()
                    .as_ref()
                    .is_some_and(|s| s.tray && s.state.stage == ConnectStage::Hosting)
            });
            if hide {
                ShowWindow(hwnd, SW_HIDE);
            } else {
                request_stop(hwnd, true);
            }
            0
        }
        WM_QUERYENDSESSION => {
            request_stop(hwnd, true);
            1
        }
        WM_ENDSESSION if wparam != 0 => {
            request_stop(hwnd, true);
            0
        }
        WM_DESTROY => {
            remove_tray(hwnd);
            PANEL.with(|slot| {
                if let Some(s) = slot.borrow_mut().as_mut() {
                    s.initialized = false;
                    s.app_stop.cancel();
                    if let Some(gate) = &s.gate {
                        gate.close();
                    }
                    if let Some(stop) = &s.stop {
                        stop.cancel();
                    }
                }
            });
            UnregisterHotKey(hwnd, HOTKEY);
            KillTimer(hwnd, TIMER);
            PostQuitMessage(0);
            0
        }
        WM_NCDESTROY => {
            LIVE.with(|live| live.set(ptr::null_mut()));
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[allow(clippy::too_many_arguments)]
async fn attempt(
    pin: String,
    generation: u64,
    prepared: Prepared,
    stop: CancellationToken,
    gate: PairingGate,
    connections: Connections,
    restart: Arc<AtomicBool>,
    tx: mpsc::Sender<Event>,
) -> std::result::Result<HostOutcome, String> {
    let progress = |text| {
        let _ = tx.send(Event::Progress { generation, text });
    };
    let cfg = prepared.cfg;
    let api = prepared.api;
    progress("Checking the connection service…");
    tokio::select! {biased;_=stop.cancelled()=>return Err(CANCELED.into()),result=check_server(&cfg)=>result?}
    progress("Checking screen capture on this PC…");
    preflight(&cfg, &stop, &restart).await?;
    let result=async{
        tokio::select!{biased;_=stop.cancelled()=>return Err(CANCELED.into()),result=api.heartbeat()=>result.map_err(|error|registration_error(&error))?}
        tokio::select!{biased;_=stop.cancelled()=>return Err(CANCELED.into()),result=api.revoke_pairing()=>result.map_err(|error|registration_error(&error))?}
        progress("Activating secure connections…");let permissions=pairing_permissions(cfg.allow_input);let grant=gate.replace().map_err(|_|CANCELED.to_string())?;
        let code=tokio::select!{biased;_=stop.cancelled()=>return Err(CANCELED.into()),result=api.activate_with_orphan_cleanup(&pin,permissions,&stop)=>result.map_err(|error|registration_error(&error))?};drop(pin);
        if stop.is_cancelled()||!gate.install(grant,code,permissions).map_err(|_|"OpenRemote could not activate access. Check this PC's clock and try again.".to_string())?{return Err(CANCELED.into());}
        let _=tx.send(Event::Ready{generation});write_diagnostic("activation_host_ready");
        host_home::serve(cfg,api.clone(),gate.clone(),stop.clone(),connections,restart).await.map_err(|error|error.to_string())
    }.await;
    gate.close();
    stop.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(3), api.revoke_pairing()).await;
    result
}
fn registration_error(error: &anyhow::Error) -> String {
    write_diagnostic("managed_connection_failed");
    if let Some(failure) = error.downcast_ref::<crate::api::ApiFailure>() {
        if failure.status == 429 {
            return "OpenRemote is busy. Wait a moment, then try again.".into();
        }
        if failure.status == 409 {
            return "Previous connections could not be cleared safely. Retry activation after they end.".into();
        }
        if matches!(failure.status, 401 | 403 | 404 | 405) {
            return "This PC could not register with its connection service. Contact the app provider.".into();
        }
    }
    "OpenRemote could not connect. Check your Internet connection and try again.".into()
}
async fn check_server(cfg: &Config) -> std::result::Result<(), String> {
    let error = || {
        "OpenRemote could not reach its service. Check your Internet connection and retry."
            .to_string()
    };
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| error())?;
    let mut response = client
        .get(format!(
            "{}/v1/info",
            cfg.api_base_url.trim_end_matches('/')
        ))
        .send()
        .await
        .map_err(|_| error())?;
    if !response.status().is_success() {
        return Err(error());
    }
    let mut bytes = vec![];
    while let Some(chunk) = response.chunk().await.map_err(|_| error())? {
        if bytes.len() + chunk.len() > 16384 {
            return Err(error());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| error())?;
    validate_activation_service(&value).map_err(|error| error.to_string())
}
async fn preflight(
    cfg: &Config,
    stop: &CancellationToken,
    restart: &AtomicBool,
) -> std::result::Result<(), String> {
    let error = || {
        "Remote desktop could not start on this PC. Reopen the app, or contact the app provider if this repeats.".to_string()
    };
    tokio::select! {
        biased;
        _ = stop.cancelled() => return Err(CANCELED.into()),
        result = media::verify_ffmpeg(&cfg.ffmpeg_path, &cfg.ffmpeg_sha256) => result.map_err(|_| error())?,
    }
    let devices = probe(
        &cfg.ffmpeg_path,
        &["-hide_banner".into(), "-nostdin".into(), "-devices".into()],
        stop,
        restart,
    )
    .await?;
    let encoders = probe(
        &cfg.ffmpeg_path,
        &["-hide_banner".into(), "-nostdin".into(), "-encoders".into()],
        stop,
        restart,
    )
    .await?;
    let component = |output: &ProbeOutput, name: &str| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .chain(String::from_utf8_lossy(&output.stderr).lines())
            .any(|line| line.split_whitespace().nth(1) == Some(name))
    };
    if !component(&devices, "gdigrab") || !component(&encoders, cfg.video_encoder.codec_name()) {
        return Err(error());
    }
    let mut args = media::ffmpeg_args(cfg, media::CaptureSource::Synthetic).map_err(|_| error())?;
    let output = args.len().checked_sub(1).ok_or_else(error)?;
    args.splice(output..output, ["-frames:v".into(), "3".into()]);
    let encoded = probe(&cfg.ffmpeg_path, &args, stop, restart).await?;
    let mut parser = media::AccessUnits::default();
    let mut frames = parser.push(&encoded.stdout).map_err(|_| error())?;
    frames.extend(parser.finish().map_err(|_| error())?);
    if frames.is_empty() {
        return Err(error());
    }
    Ok(())
}
struct ProbeOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}
async fn probe(
    path: &Path,
    args: &[String],
    stop: &CancellationToken,
    restart: &AtomicBool,
) -> std::result::Result<ProbeOutput, String> {
    use std::process::Stdio;
    let error = || {
        "Remote desktop could not start on this PC. Reopen the app, or contact the app provider if this repeats.".to_string()
    };
    if stop.is_cancelled() {
        return Err(CANCELED.into());
    }
    let mut command = Command::new(path);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .creation_flags(0x08000000)
        .env_remove("OPENREMOTE_ENROLLMENT_TOKEN");
    if let Some(root) = std::env::var_os("SystemRoot") {
        command.env(
            "PATH",
            format!(
                "{}\\System32;{}",
                root.to_string_lossy(),
                root.to_string_lossy()
            ),
        );
    }
    let mut child = command.spawn().map_err(|_| error())?;
    let stdout = child.stdout.take().ok_or_else(error)?;
    let stderr = child.stderr.take().ok_or_else(error)?;
    let work = async {
        let mut out = vec![];
        let mut err = vec![];
        let read_out = async { stdout.take(2_097_153).read_to_end(&mut out).await };
        let read_err = async { stderr.take(262_145).read_to_end(&mut err).await };
        let (a, b) = tokio::join!(read_out, read_err);
        a.map_err(|_| error())?;
        b.map_err(|_| error())?;
        if out.len() > 2_097_152 || err.len() > 262_144 {
            return Err(error());
        }
        if !child.wait().await.map_err(|_| error())?.success() {
            return Err(error());
        }
        Ok(ProbeOutput {
            stdout: out,
            stderr: err,
        })
    };
    let result = tokio::select! {biased;_=stop.cancelled()=>Err(CANCELED.into()),r=tokio::time::timeout(Duration::from_secs(15),work)=>r.unwrap_or_else(|_|Err(error()))};
    if result.is_err() {
        let _ = child.start_kill();
        if !matches!(
            tokio::time::timeout(Duration::from_secs(2), child.wait()).await,
            Ok(Ok(_))
        ) {
            restart.store(true, Ordering::SeqCst);
        }
        write_diagnostic("managed_encoder_check_failed");
    }
    result
}
