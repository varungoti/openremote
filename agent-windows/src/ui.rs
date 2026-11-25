//! Native, owner-visible consent and a persistent local disconnect window.
use crate::protocol::PermissionSet;
use anyhow::{bail, Result};
use tokio_util::sync::CancellationToken;

pub fn approve(
    controller: &str,
    fingerprint: &str,
    permissions: PermissionSet,
    display: &str,
    stop: CancellationToken,
) -> Result<bool> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Graphics::Gdi::COLOR_WINDOW, System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::*,
        };
        if stop.is_cancelled() {
            return Ok(false);
        }
        let name: String = controller
            .chars()
            .filter(|c| !c.is_control())
            .take(80)
            .collect();
        PROMPT.with(|v| {
            *v.borrow_mut() = Some(Prompt {
                stop,
                deadline: std::time::Instant::now() + std::time::Duration::from_secs(60),
                approved: false,
            })
        });
        let instance = GetModuleHandleW(std::ptr::null());
        let class = wide("OpenRemoteConsent");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(consent_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_WINDOW + 1) as _,
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST,
            class.as_ptr(),
            wide("OpenRemote — connection request").as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            610,
            420,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            bail!("failed to create local consent window");
        }
        let text=wide(&format!("{name} requests access to this PC.\n\nCompare this fingerprint with Settings on your phone:\n{fingerprint}\n\nDisplay: {display}\n{}\n\nAllow only if this is your intended phone. This request expires in 60 seconds.", permission_description(permissions)));
        CreateWindowExW(
            0,
            wide("STATIC").as_ptr(),
            text.as_ptr(),
            WS_CHILD | WS_VISIBLE,
            16,
            16,
            565,
            300,
            hwnd,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        CreateWindowExW(
            0,
            wide("BUTTON").as_ptr(),
            wide("Allow once").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            16,
            335,
            200,
            32,
            hwnd,
            1usize as _,
            instance,
            std::ptr::null(),
        );
        CreateWindowExW(
            0,
            wide("BUTTON").as_ptr(),
            wide("Deny").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            230,
            335,
            180,
            32,
            hwnd,
            2usize as _,
            instance,
            std::ptr::null(),
        );
        SetTimer(hwnd, 1, 250, None);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        Ok(PROMPT.with(|v| {
            v.borrow_mut()
                .take()
                .map(|v| v.approved && !v.stop.is_cancelled())
                .unwrap_or(false)
        }))
    }
    #[cfg(not(windows))]
    {
        let _ = (controller, fingerprint, permissions, display, stop);
        bail!("desktop consent requires Windows");
    }
}

pub fn permission_description(permissions: PermissionSet) -> String {
    format!("Permissions granted for this connection:\nView selected monitor: {}\nKeyboard and mouse control: {}\nRead / replace PC text clipboard: {}\nUpload files to the PC's ReceivedFiles folder: {}",
        if permissions.view_screen {"Yes"} else {"No"},
        if permissions.control_input {"Yes"} else {"No"},
        if permissions.clipboard {"Yes"} else {"No"},
        if permissions.file_transfer {"Yes"} else {"No"})
}

#[cfg(windows)]
struct Prompt {
    stop: CancellationToken,
    deadline: std::time::Instant,
    approved: bool,
}
#[cfg(windows)]
thread_local! {static PROMPT:std::cell::RefCell<Option<Prompt>>=const {std::cell::RefCell::new(None)};}
#[cfg(windows)]
unsafe extern "system" fn consent_proc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    match msg {
        WM_COMMAND => {
            PROMPT.with(|v| {
                if let Some(p) = v.borrow_mut().as_mut() {
                    p.approved = (wparam & 0xffff) == 1
                        && !p.stop.is_cancelled()
                        && std::time::Instant::now() < p.deadline;
                }
            });
            DestroyWindow(hwnd);
            0
        }
        WM_TIMER => {
            let expired = PROMPT.with(|v| {
                v.borrow().as_ref().is_none_or(|p| {
                    p.stop.is_cancelled() || std::time::Instant::now() >= p.deadline
                })
            });
            if expired {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            KillTimer(hwnd, 1);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub struct Notice {
    #[cfg(windows)]
    hwnd: isize,
}
impl Notice {
    pub fn show(controller: &str, display: &str, stop: CancellationToken) -> Result<Self> {
        #[cfg(windows)]
        {
            let name: String = controller
                .chars()
                .filter(|c| !c.is_control())
                .take(80)
                .collect();
            let display = display.to_owned();
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            std::thread::spawn(move || unsafe {
                use windows_sys::Win32::{
                    Graphics::Gdi::COLOR_WINDOW, System::LibraryLoader::GetModuleHandleW,
                    UI::WindowsAndMessaging::*,
                };
                CANCEL.with(|v| *v.borrow_mut() = Some(stop.clone()));
                let instance = GetModuleHandleW(std::ptr::null());
                let class = wide("OpenRemoteSessionNotice");
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(window_proc),
                    hInstance: instance,
                    lpszClassName: class.as_ptr(),
                    hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
                    hbrBackground: (COLOR_WINDOW + 1) as _,
                    ..std::mem::zeroed()
                };
                RegisterClassW(&wc);
                let hwnd = CreateWindowExW(
                    WS_EX_TOPMOST,
                    class.as_ptr(),
                    wide("OpenRemote — remote session").as_ptr(),
                    WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
                    CW_USEDEFAULT,
                    CW_USEDEFAULT,
                    510,
                    265,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    instance,
                    std::ptr::null(),
                );
                if hwnd.is_null() {
                    stop.cancel();
                    let _ = tx.send(0isize);
                    return;
                }
                let text=wide(&format!("Remote access accepted for {name}.\nDisplay: {display}\nThis window stays visible throughout the session.\nClose it or use Disconnect to stop screen sharing and input.\n\nEmergency stop: Ctrl + Alt + F12"));
                CreateWindowExW(
                    0,
                    wide("STATIC").as_ptr(),
                    text.as_ptr(),
                    WS_CHILD | WS_VISIBLE,
                    16,
                    16,
                    465,
                    140,
                    hwnd,
                    std::ptr::null_mut(),
                    instance,
                    std::ptr::null(),
                );
                CreateWindowExW(
                    0,
                    wide("BUTTON").as_ptr(),
                    wide("Disconnect now").as_ptr(),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                    16,
                    170,
                    200,
                    32,
                    hwnd,
                    1usize as _,
                    instance,
                    std::ptr::null(),
                );
                use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
                    RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, VK_F12,
                };
                RegisterHotKey(hwnd, 1, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, VK_F12 as u32);
                let _ = tx.send(hwnd as isize);
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                UnregisterHotKey(hwnd, 1);
                stop.cancel();
            });
            let hwnd = rx.recv_timeout(std::time::Duration::from_secs(5))?;
            if hwnd == 0 {
                bail!("failed to create visible session notice");
            }
            Ok(Self { hwnd })
        }
        #[cfg(not(windows))]
        {
            let _ = (controller, display, stop);
            bail!("session notice requires Windows");
        }
    }
}
impl Drop for Notice {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                self.hwnd as _,
                windows_sys::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                0,
                0,
            );
        }
    }
}

#[cfg(windows)]
thread_local! {static CANCEL:std::cell::RefCell<Option<CancellationToken>>=const {std::cell::RefCell::new(None)};}
#[cfg(windows)]
unsafe extern "system" fn window_proc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    match msg {
        WM_COMMAND | WM_HOTKEY | WM_CLOSE => {
            // Revoke permission before destroying the notice, so a queued remote input cannot
            // execute once the local owner has clicked Stop.
            CANCEL.with(|v| {
                if let Some(c) = v.borrow().as_ref() {
                    c.cancel();
                }
            });
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
