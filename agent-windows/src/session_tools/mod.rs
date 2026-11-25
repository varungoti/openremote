//! Explicit, separately permissioned clipboard and upload operations. Errors in
//! this channel never authorize input or end an otherwise healthy desktop.
mod clipboard;
pub mod protocol;
mod uploads;

use crate::protocol::PermissionSet;
use clipboard::{Clipboard, NativeClipboard};
use protocol::{InvalidFrame, Operation, Request, MAX_FRAME};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
use uploads::{InternetZone, Uploads};

/// Fixed public errors never include OS paths, received data or secrets.
#[derive(Debug, Clone, Copy)]
struct ToolError {
    code: &'static str,
    message: &'static str,
}
impl ToolError {
    const fn new(code: &'static str, message: &'static str) -> Self {
        Self { code, message }
    }
}

const MAX_REQUESTS: usize = 65_536;
const MAX_PER_SECOND: u32 = 128;

pub struct ToolSession {
    state: Mutex<State>,
    closed: AtomicBool,
    session_stop: CancellationToken,
}
struct State {
    permissions: PermissionSet,
    clipboard: Box<dyn Clipboard>,
    uploads: Uploads,
    seen_requests: HashSet<String>,
    rate_start: Instant,
    rate_count: u32,
}
impl ToolSession {
    pub fn new(permissions: PermissionSet, session_stop: CancellationToken) -> Self {
        Self::with_backends(
            permissions,
            session_stop,
            Box::new(NativeClipboard),
            Uploads::new(
                crate::config::data_dir().join("ReceivedFiles"),
                Box::new(InternetZone),
            ),
        )
    }
    fn with_backends(
        permissions: PermissionSet,
        session_stop: CancellationToken,
        clipboard: Box<dyn Clipboard>,
        uploads: Uploads,
    ) -> Self {
        Self {
            state: Mutex::new(State {
                permissions,
                clipboard,
                uploads,
                seen_requests: HashSet::new(),
                rate_start: Instant::now(),
                rate_count: 0,
            }),
            closed: AtomicBool::new(false),
            session_stop,
        }
    }
    /// Run on a blocking worker: Win32 clipboard and local file I/O must never
    /// stall RTP handling. A framing error closes only the tools channel.
    pub fn process(&self, bytes: &[u8]) -> Result<String, InvalidFrame> {
        let Request { id, op } = match protocol::parse(bytes) {
            Ok(request) => request,
            Err(error) => {
                self.revoke();
                return Err(error);
            }
        };
        let mut state = self.state.lock().map_err(|_| InvalidFrame)?;
        if self.is_closed() {
            state.uploads.cancel_all();
            return Err(InvalidFrame);
        }
        let now = Instant::now();
        state.uploads.expire(now);
        if now.duration_since(state.rate_start) >= Duration::from_secs(1) {
            state.rate_start = now;
            state.rate_count = 0;
        }
        state.rate_count += 1;
        if state.rate_count > MAX_PER_SECOND
            || state.seen_requests.len() >= MAX_REQUESTS
            || !state.seen_requests.insert(id.clone())
        {
            state.uploads.cancel_all();
            self.closed.store(true, Ordering::SeqCst);
            return Err(InvalidFrame);
        }
        let result = state.apply(op, || !self.is_closed());
        // A native operation may finish after local stop or the channel's I/O
        // deadline. Discard its response and clean any remaining partial.
        if self.is_closed() {
            state.uploads.cancel_all();
            return Err(InvalidFrame);
        }
        state.uploads.commit_finished();
        Ok(response(&id, result))
    }
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst) || self.session_stop.is_cancelled()
    }
    /// Revoke immediately without waiting on a potentially busy Win32 call.
    /// An already-running worker checks the flag again before returning.
    pub fn revoke(&self) {
        self.closed.store(true, Ordering::SeqCst);
        if let Ok(mut state) = self.state.try_lock() {
            state.uploads.cancel_all();
        }
    }
    pub fn maintenance(&self) {
        if let Ok(mut state) = self.state.try_lock() {
            if self.is_closed() {
                state.uploads.cancel_all();
            } else {
                state.uploads.expire(Instant::now());
            }
        }
    }
}
impl Drop for ToolSession {
    fn drop(&mut self) {
        self.revoke();
    }
}
impl State {
    fn apply(
        &mut self,
        op: Operation,
        still_allowed: impl Fn() -> bool,
    ) -> Result<Value, ToolError> {
        match &op {
            Operation::ClipboardRead | Operation::ClipboardWrite(_)
                if !self.permissions.clipboard =>
            {
                return Err(ToolError::new(
                    "permission_denied",
                    "Clipboard is not permitted for this session.",
                ));
            }
            Operation::FileBegin { .. }
            | Operation::FileChunk { .. }
            | Operation::FileFinish(_)
            | Operation::FileCancel(_)
                if !self.permissions.file_transfer =>
            {
                return Err(ToolError::new(
                    "permission_denied",
                    "File transfer is not permitted for this session.",
                ));
            }
            _ => {}
        }
        match op {
            Operation::ClipboardRead => {
                let text = self.clipboard.read()?;
                if !protocol::valid_text(&text) {
                    return Err(ToolError::new(
                        "clipboard_text_limit",
                        "Clipboard text exceeds the supported UTF-8 limit or contains NUL.",
                    ));
                }
                Ok(json!({"text":text}))
            }
            Operation::ClipboardWrite(text) => {
                self.clipboard.write(&text)?;
                Ok(json!({}))
            }
            Operation::FileBegin {
                id,
                name,
                size,
                sha256,
            } => self.uploads.begin(id, name, size, sha256),
            Operation::FileChunk { id, offset, data } => self.uploads.chunk(&id, offset, &data),
            Operation::FileFinish(id) => self.uploads.finish(&id, still_allowed),
            Operation::FileCancel(id) => self.uploads.cancel(&id),
        }
    }
}
fn response(id: &str, result: Result<Value, ToolError>) -> String {
    let value = match result {
        Ok(value) => json!({"v":1,"type":"tool_result","requestId":id,"ok":true,"result":value}),
        Err(error) => {
            json!({"v":1,"type":"tool_result","requestId":id,"ok":false,"error":{"code":error.code,"message":error.message}})
        }
    };
    let encoded = value.to_string();
    if encoded.len() > MAX_FRAME {
        // Control characters can expand sixfold in JSON even when the original
        // clipboard string fits its independent UTF-8 byte bound.
        return json!({"v":1,"type":"tool_result","requestId":id,"ok":false,"error":{"code":"response_too_large","message":"The clipboard text cannot fit in one supported response frame."}}).to_string();
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::PathBuf,
        sync::{atomic::AtomicUsize, Arc},
    };
    struct MemoryClipboard {
        value: String,
        reads: Arc<AtomicUsize>,
        writes: Arc<AtomicUsize>,
    }
    impl Clipboard for MemoryClipboard {
        fn read(&mut self) -> Result<String, ToolError> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(self.value.clone())
        }
        fn write(&mut self, text: &str) -> Result<(), ToolError> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            self.value = text.into();
            Ok(())
        }
    }
    fn session(
        permissions: PermissionSet,
        text: &str,
    ) -> (ToolSession, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let reads = Arc::new(AtomicUsize::new(0));
        let writes = Arc::new(AtomicUsize::new(0));
        let clipboard = Box::new(MemoryClipboard {
            value: text.into(),
            reads: reads.clone(),
            writes: writes.clone(),
        });
        let session = ToolSession::with_backends(
            permissions,
            CancellationToken::new(),
            clipboard,
            Uploads::new(PathBuf::from("must-not-be-created"), Box::new(InternetZone)),
        );
        (session, reads, writes)
    }
    fn call(s: &ToolSession, n: u128, op: &str, fields: Value) -> Value {
        let mut v = json!({"v":1,"type":op,"requestId":format!("{n:032x}")});
        if let Some(fields) = fields.as_object() {
            for (k, value) in fields {
                v[k] = value.clone();
            }
        }
        serde_json::from_str(&s.process(v.to_string().as_bytes()).unwrap()).unwrap()
    }
    #[test]
    fn permission_denial_has_no_clipboard_or_file_side_effects_and_session_stays_open() {
        let (s, reads, writes) = session(PermissionSet::default(), "private");
        for (n, op, fields) in [
            (1, "clipboard_read", json!({})),
            (2, "clipboard_write", json!({"text":"change"})),
            (
                3,
                "file_begin",
                json!({"transferId":"a".repeat(32),"name":"x","size":0,"sha256":"0".repeat(64)}),
            ),
        ] {
            let r = call(&s, n, op, fields);
            assert_eq!(r["ok"], false);
            assert_eq!(r["error"]["code"], "permission_denied");
            assert!(!r.to_string().contains("private"));
        }
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        assert_eq!(writes.load(Ordering::SeqCst), 0);
        assert!(!s.is_closed());
    }
    #[test]
    fn clipboard_operations_are_explicit_and_empty_write_is_supported() {
        let (s, reads, writes) = session(
            PermissionSet {
                clipboard: true,
                ..Default::default()
            },
            "before",
        );
        s.maintenance();
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        assert_eq!(
            call(&s, 1, "clipboard_read", json!({}))["result"]["text"],
            "before"
        );
        assert_eq!(
            call(&s, 2, "clipboard_write", json!({"text":""}))["result"],
            json!({})
        );
        assert_eq!(
            call(&s, 3, "clipboard_read", json!({}))["result"]["text"],
            ""
        );
        assert_eq!(reads.load(Ordering::SeqCst), 2);
        assert_eq!(writes.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn oversized_and_escaped_clipboard_responses_return_bounded_errors() {
        for (text, code) in [
            ("é".repeat(2049), "clipboard_text_limit"),
            ("\u{0001}".repeat(4096), "response_too_large"),
        ] {
            let (s, _, _) = session(
                PermissionSet {
                    clipboard: true,
                    ..Default::default()
                },
                &text,
            );
            let response = call(&s, 1, "clipboard_read", json!({}));
            assert_eq!(response["error"]["code"], code);
            assert!(response.to_string().len() <= MAX_FRAME);
            assert!(!s.is_closed());
        }
    }
    #[test]
    fn duplicate_request_ids_cannot_replay_a_side_effect() {
        let (s, _, writes) = session(
            PermissionSet {
                clipboard: true,
                ..Default::default()
            },
            "",
        );
        let request =
            json!({"v":1,"type":"clipboard_write","requestId":"a".repeat(32),"text":"once"})
                .to_string();
        assert!(s.process(request.as_bytes()).is_ok());
        assert!(s.process(request.as_bytes()).is_err());
        assert!(s.is_closed());
        assert_eq!(writes.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn rate_limit_and_revocation_prevent_more_operations() {
        let (s, reads, _) = session(
            PermissionSet {
                clipboard: true,
                ..Default::default()
            },
            "",
        );
        {
            let mut state = s.state.lock().unwrap();
            state.rate_count = MAX_PER_SECOND;
        }
        assert!(s
            .process(
                json!({"v":1,"type":"clipboard_read","requestId":"b".repeat(32)})
                    .to_string()
                    .as_bytes()
            )
            .is_err());
        assert!(s.is_closed());
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        let (s, reads, _) = session(
            PermissionSet {
                clipboard: true,
                ..Default::default()
            },
            "",
        );
        s.session_stop.cancel();
        assert!(s
            .process(
                json!({"v":1,"type":"clipboard_read","requestId":"c".repeat(32)})
                    .to_string()
                    .as_bytes()
            )
            .is_err());
        assert_eq!(reads.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn revocation_during_native_finish_discards_file_and_never_returns_success() {
        use std::{fs, sync::mpsc};
        struct PausedZone {
            started: mpsc::SyncSender<()>,
            resume: mpsc::Receiver<()>,
        }
        impl uploads::ZoneMarker for PausedZone {
            fn mark(&self, _: &std::path::Path, _: &fs::File) -> Result<(), ToolError> {
                self.started.send(()).unwrap();
                self.resume.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(())
            }
        }
        let root = std::env::temp_dir().join(format!(
            "openremote-revoke-test-{:032x}",
            rand::random::<u128>()
        ));
        fs::create_dir(&root).unwrap();
        let received = root.join("ReceivedFiles");
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (resume_tx, resume_rx) = mpsc::sync_channel(1);
        let clipboard = Box::new(MemoryClipboard {
            value: String::new(),
            reads: Arc::new(AtomicUsize::new(0)),
            writes: Arc::new(AtomicUsize::new(0)),
        });
        let s = Arc::new(ToolSession::with_backends(
            PermissionSet {
                file_transfer: true,
                ..Default::default()
            },
            CancellationToken::new(),
            clipboard,
            Uploads::new(
                received.clone(),
                Box::new(PausedZone {
                    started: started_tx,
                    resume: resume_rx,
                }),
            ),
        ));
        assert_eq!(
            call(
                &s,
                1,
                "file_begin",
                json!({"transferId":"a".repeat(32),"name":"empty.txt","size":0,"sha256":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"})
            )["ok"],
            true
        );
        let worker = s.clone();
        let task = std::thread::spawn(move || {
            worker.process(json!({"v":1,"type":"file_finish","requestId":"b".repeat(32),"transferId":"a".repeat(32)}).to_string().as_bytes())
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        s.revoke(); // state mutex is held by the worker, exactly as during Win32 I/O.
        resume_tx.send(()).unwrap();
        assert!(task.join().unwrap().is_err());
        assert!(s.is_closed());
        assert_eq!(fs::read_dir(received).unwrap().count(), 0);
        drop(s);
        fs::remove_dir_all(root).unwrap();
    }
}
