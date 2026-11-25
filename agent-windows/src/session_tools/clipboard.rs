//! One-shot Unicode clipboard access. There is no polling or automatic sync.
use super::{protocol::valid_text, ToolError};
type Result<T> = std::result::Result<T, ToolError>;

pub(super) trait Clipboard: Send {
    fn read(&mut self) -> Result<String>;
    fn write(&mut self, text: &str) -> Result<()>;
}
pub(super) struct NativeClipboard;
impl Clipboard for NativeClipboard {
    fn read(&mut self) -> Result<String> {
        #[cfg(windows)]
        {
            windows::read()
        }
        #[cfg(not(windows))]
        {
            Err(ToolError::new(
                "clipboard_unavailable",
                "Native clipboard access requires Windows.",
            ))
        }
    }
    fn write(&mut self, text: &str) -> Result<()> {
        if !valid_text(text) {
            return Err(invalid_text());
        }
        #[cfg(windows)]
        {
            windows::write(text)
        }
        #[cfg(not(windows))]
        {
            Err(ToolError::new(
                "clipboard_unavailable",
                "Native clipboard access requires Windows.",
            ))
        }
    }
}
fn invalid_text() -> ToolError {
    ToolError::new(
        "clipboard_text_limit",
        "Clipboard text must contain at most 4096 UTF-8 bytes and no NUL.",
    )
}

#[cfg(windows)]
mod windows {
    use super::*;
    use crate::session_tools::protocol::MAX_TEXT;
    use std::ptr;
    use windows_sys::Win32::{
        Foundation::{GlobalFree, HGLOBAL, HWND},
        System::{
            DataExchange::*, LibraryLoader::GetModuleHandleW, Memory::*, Ole::CF_UNICODETEXT,
        },
        UI::WindowsAndMessaging::*,
    };
    fn unavailable() -> ToolError {
        ToolError::new(
            "clipboard_unavailable",
            "The PC clipboard is busy or unavailable.",
        )
    }
    struct Owner(HWND);
    impl Owner {
        fn new() -> Result<Self> {
            // EmptyClipboard must have a real owner HWND for SetClipboardData.
            // The session's visible disconnect window remains separate; this
            // message-only window is only the native clipboard resource owner.
            let class: Vec<u16> = "STATIC".encode_utf16().chain(Some(0)).collect();
            let name: Vec<u16> = "OpenRemoteClipboard"
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    name.as_ptr(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE,
                    ptr::null_mut(),
                    GetModuleHandleW(ptr::null()),
                    ptr::null(),
                )
            };
            if hwnd.is_null() {
                return Err(unavailable());
            }
            Ok(Self(hwnd))
        }
    }
    impl Drop for Owner {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }
    struct OpenClipboardGuard;
    impl OpenClipboardGuard {
        fn open(owner: &Owner) -> Result<Self> {
            if unsafe { OpenClipboard(owner.0) } == 0 {
                return Err(unavailable());
            }
            Ok(Self)
        }
    }
    impl Drop for OpenClipboardGuard {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }
    struct GlobalLockGuard(HGLOBAL);
    impl Drop for GlobalLockGuard {
        fn drop(&mut self) {
            unsafe {
                GlobalUnlock(self.0);
            }
        }
    }
    struct OwnedMemory(HGLOBAL);
    impl Drop for OwnedMemory {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    GlobalFree(self.0);
                }
            }
        }
    }

    pub fn read() -> Result<String> {
        let owner = Owner::new()?;
        let _open = OpenClipboardGuard::open(&owner)?;
        if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT as u32) } == 0 {
            return Err(ToolError::new(
                "clipboard_no_text",
                "The PC clipboard does not contain text.",
            ));
        }
        let memory = unsafe { GetClipboardData(CF_UNICODETEXT as u32) };
        if memory.is_null() {
            return Err(unavailable());
        }
        let size = unsafe { GlobalSize(memory) };
        if size < 2 || size % 2 != 0 {
            return Err(invalid_text());
        }
        let data = unsafe { GlobalLock(memory) } as *const u16;
        if data.is_null() {
            return Err(unavailable());
        }
        let _locked = GlobalLockGuard(memory);
        // A large allocation may contain short text. Inspect a bounded prefix,
        // require its terminator, then enforce UTF-8 bytes after UTF-16 decoding.
        let units = unsafe { std::slice::from_raw_parts(data, (size / 2).min(MAX_TEXT + 1)) };
        let end = units
            .iter()
            .position(|v| *v == 0)
            .ok_or_else(invalid_text)?;
        let text = String::from_utf16(&units[..end]).map_err(|_| invalid_text())?;
        if !valid_text(&text) {
            return Err(invalid_text());
        }
        Ok(text)
    }
    pub fn write(text: &str) -> Result<()> {
        if !valid_text(text) {
            return Err(invalid_text());
        }
        let content: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, content.len() * 2) };
        if memory.is_null() {
            return Err(unavailable());
        }
        let mut owned = OwnedMemory(memory);
        let data = unsafe { GlobalLock(memory) } as *mut u16;
        if data.is_null() {
            return Err(unavailable());
        }
        {
            let _locked = GlobalLockGuard(memory);
            unsafe {
                ptr::copy_nonoverlapping(content.as_ptr(), data, content.len());
            }
        }
        // Allocate/copy before touching the existing clipboard. Do not empty it
        // if allocation or opening fails. Windows owns the HGLOBAL on success.
        let owner = Owner::new()?;
        let _open = OpenClipboardGuard::open(&owner)?;
        if unsafe { EmptyClipboard() } == 0 {
            return Err(unavailable());
        }
        if unsafe { SetClipboardData(CF_UNICODETEXT as u32, memory) }.is_null() {
            return Err(unavailable());
        }
        owned.0 = ptr::null_mut();
        Ok(())
    }
}
