//! A single, bounded upload into the owner's fixed receive directory.
//! The remote peer can supply a display basename, never a destination path.
use super::{
    protocol::{lower_hex, valid_name, MAX_CHUNK, MAX_FILE, SESSION_QUOTA},
    ToolError,
};
use rand::{rngs::OsRng, RngCore};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const IDLE: Duration = Duration::from_secs(60);
type Result<T> = std::result::Result<T, ToolError>;

pub(super) trait ZoneMarker: Send {
    fn mark(&self, path: &Path, file: &File) -> Result<()>;
}
pub(super) struct InternetZone;
impl ZoneMarker for InternetZone {
    fn mark(&self, path: &Path, file: &File) -> Result<()> {
        #[cfg(windows)]
        {
            windows::mark_internet_zone(path, file)
        }
        #[cfg(not(windows))]
        {
            let _ = (path, file);
            Err(ToolError::new(
                "unsupported_filesystem",
                "Internet-zone metadata requires a supported Windows filesystem.",
            ))
        }
    }
}

pub(super) struct Uploads {
    root: PathBuf,
    area: Option<ReceiveArea>,
    active: Option<Partial>,
    finished: Option<Partial>,
    reserved: u64,
    transfer_ids: HashSet<String>,
    marker: Box<dyn ZoneMarker>,
}
impl Uploads {
    pub fn new(root: PathBuf, marker: Box<dyn ZoneMarker>) -> Self {
        Self {
            root,
            area: None,
            active: None,
            finished: None,
            reserved: 0,
            transfer_ids: HashSet::new(),
            marker,
        }
    }
    pub fn begin(&mut self, id: String, name: String, size: u64, sha256: String) -> Result<Value> {
        if !lower_hex(&id, 32) || !valid_name(&name) || size > MAX_FILE || !lower_hex(&sha256, 64) {
            return Err(ToolError::new(
                "invalid_request",
                "Upload metadata is invalid.",
            ));
        }
        if self.active.is_some() || self.finished.is_some() {
            return Err(ToolError::new(
                "transfer_busy",
                "An upload is already in progress.",
            ));
        }
        if self.transfer_ids.contains(&id) {
            return Err(ToolError::new(
                "duplicate_transfer",
                "Use a new transfer identifier.",
            ));
        }
        if size > SESSION_QUOTA.saturating_sub(self.reserved) {
            return Err(ToolError::new(
                "quota_exceeded",
                "The session upload quota is exhausted.",
            ));
        }
        if self.area.is_none() {
            self.area = Some(ReceiveArea::open(&self.root)?);
        }
        let area = self.area.as_ref().expect("area initialized");
        let saved_name = format!("{id}_{name}");
        match fs::symlink_metadata(area.path.join(&saved_name)) {
            Ok(_) => {
                return Err(ToolError::new(
                    "already_exists",
                    "A file with this transfer name already exists.",
                ))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(io_error()),
        }
        let mut nonce = [0u8; 16];
        OsRng.fill_bytes(&mut nonce);
        let nonce: String = nonce.iter().map(|v| format!("{v:02x}")).collect();
        let temp_path = area.path.join(format!(".openremote-{id}-{nonce}.part"));
        let file = private_file(&temp_path).map_err(|_| io_error())?;
        self.reserved += size;
        self.transfer_ids.insert(id.clone());
        self.active = Some(Partial {
            id: id.clone(),
            saved_name,
            size,
            expected: sha256,
            received: 0,
            hash: Sha256::new(),
            file: Some(file),
            path: temp_path,
            last_activity: Instant::now(),
            committed: false,
        });
        Ok(json!({"transferId":id,"received":0,"chunkBytes":MAX_CHUNK}))
    }
    pub fn chunk(&mut self, id: &str, offset: u64, data: &[u8]) -> Result<Value> {
        let Some(partial) = self.active.as_mut().filter(|p| p.id == id) else {
            return Err(no_transfer());
        };
        if data.len() > MAX_CHUNK || offset != partial.received {
            self.cancel_all();
            return Err(ToolError::new(
                "offset_mismatch",
                "The upload offset is invalid; the incomplete file was removed.",
            ));
        }
        if data.len() as u64 > partial.size.saturating_sub(partial.received) {
            self.cancel_all();
            return Err(ToolError::new(
                "length_mismatch",
                "The upload exceeds its declared size; the incomplete file was removed.",
            ));
        }
        if partial
            .file
            .as_mut()
            .expect("open partial")
            .write_all(data)
            .is_err()
        {
            self.cancel_all();
            return Err(io_error());
        }
        partial.hash.update(data);
        partial.received += data.len() as u64;
        partial.last_activity = Instant::now();
        Ok(json!({"transferId":id,"received":partial.received}))
    }
    pub fn finish(&mut self, id: &str, still_allowed: impl Fn() -> bool) -> Result<Value> {
        if !self.active.as_ref().is_some_and(|p| p.id == id) {
            return Err(no_transfer());
        }
        // Taking ownership makes every failure below delete the partial via Drop.
        let mut partial = self.active.take().expect("matching partial");
        if partial.received != partial.size {
            return Err(ToolError::new(
                "length_mismatch",
                "The upload length does not match; the incomplete file was removed.",
            ));
        }
        let digest = format!("{:x}", partial.hash.clone().finalize());
        if digest != partial.expected {
            return Err(ToolError::new(
                "hash_mismatch",
                "The upload digest does not match; the incomplete file was removed.",
            ));
        }
        let file = partial.file.as_ref().expect("open partial");
        file.sync_all().map_err(|_| io_error())?;
        self.marker.mark(&partial.path, file)?;
        if !still_allowed() {
            return Err(ToolError::new(
                "cancelled",
                "The transfer was cancelled before publication.",
            ));
        }
        let destination = self
            .area
            .as_ref()
            .expect("active area")
            .path
            .join(&partial.saved_name);
        publish(file, &partial.path, &destination).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                ToolError::new(
                    "already_exists",
                    "A file with this transfer name already exists.",
                )
            } else {
                io_error()
            }
        })?;
        partial.path = destination;
        let result =
            json!({"transferId":id,"name":partial.saved_name,"size":partial.size,"sha256":digest});
        // Keep the exclusive rollback guard until the dispatcher performs its
        // final authorization check. Revocation during native I/O can therefore
        // remove this exact file even if its rename already succeeded.
        self.finished = Some(partial);
        Ok(result)
    }
    pub fn commit_finished(&mut self) {
        if let Some(mut partial) = self.finished.take() {
            partial.committed = true;
        }
    }
    pub fn cancel(&mut self, id: &str) -> Result<Value> {
        if self.active.as_ref().is_some_and(|p| p.id == id) {
            self.cancel_all();
        }
        // Idempotent cancellation lets a phone cancel after a response timeout,
        // including when an earlier failure has already removed the partial.
        Ok(json!({"transferId":id}))
    }
    pub fn cancel_all(&mut self) {
        self.active.take();
        self.finished.take();
    }
    pub fn expire(&mut self, now: Instant) {
        if self
            .active
            .as_ref()
            .is_some_and(|p| now.saturating_duration_since(p.last_activity) >= IDLE)
        {
            self.cancel_all();
        }
    }
}
impl Drop for Uploads {
    fn drop(&mut self) {
        self.cancel_all();
    }
}

struct Partial {
    id: String,
    saved_name: String,
    size: u64,
    expected: String,
    received: u64,
    hash: Sha256,
    file: Option<File>,
    path: PathBuf,
    last_activity: Instant,
    committed: bool,
}
impl Drop for Partial {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        #[cfg(windows)]
        if let Some(file) = self.file.as_ref() {
            // Delete the exact still-exclusive file, not a replaceable path.
            if windows::delete_on_close(file).is_ok() {
                self.file.take();
                return;
            }
        }
        self.file.take();
        let _ = fs::remove_file(&self.path);
    }
}

struct ReceiveArea {
    path: PathBuf,
    // On Windows, neither directory handle shares DELETE access. They keep the
    // checked application and receive directories from being renamed/replaced.
    _parent: File,
    _directory: File,
}
impl ReceiveArea {
    fn open(path: &Path) -> Result<Self> {
        if !path.is_absolute() {
            return Err(unsafe_directory());
        }
        let parent_path = path.parent().ok_or_else(unsafe_directory)?;
        let parent = open_directory(parent_path)?;
        match create_directory(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(io_error()),
        }
        let directory = open_directory(path)?;
        Ok(Self {
            path: path.to_owned(),
            _parent: parent,
            _directory: directory,
        })
    }
}
fn open_directory(path: &Path) -> Result<File> {
    // symlink_metadata rejects symlinks before open on the Linux test backend;
    // the production Windows open additionally disables reparse traversal and
    // checks the actual handle attributes after opening.
    let metadata = fs::symlink_metadata(path).map_err(|_| unsafe_directory())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(unsafe_directory());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::*;
        options
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path).map_err(|_| unsafe_directory())?;
    let actual = file.metadata().map_err(|_| unsafe_directory())?;
    if !actual.is_dir() {
        return Err(unsafe_directory());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        if actual.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(unsafe_directory());
        }
    }
    Ok(file)
}
fn private_file(path: &Path) -> std::io::Result<File> {
    #[cfg(windows)]
    {
        windows::create_private_file(path)
    }
    #[cfg(not(windows))]
    {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(path)
    }
}
fn create_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        windows::create_private_directory(path)
    }
    #[cfg(not(windows))]
    {
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path)
    }
}
fn publish(file: &File, temporary: &Path, destination: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        let _ = temporary;
        windows::publish(file, destination)
    }
    #[cfg(not(windows))]
    {
        let _ = file;
        // hard_link is exclusive; std::fs::rename would silently overwrite on Unix.
        fs::hard_link(temporary, destination)?;
        if let Err(error) = fs::remove_file(temporary) {
            let _ = fs::remove_file(destination);
            return Err(error);
        }
        Ok(())
    }
}
fn io_error() -> ToolError {
    ToolError::new("file_io", "The PC could not safely store this file.")
}
fn unsafe_directory() -> ToolError {
    ToolError::new("unsafe_directory", "The receive directory must be a normal local directory, without a reparse point or symlink.")
}
fn no_transfer() -> ToolError {
    ToolError::new(
        "unknown_transfer",
        "This upload is not active; start a new transfer.",
    )
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{
        ffi::OsString,
        io::{Read, Seek, SeekFrom},
        mem::{align_of, offset_of, size_of},
        os::windows::{
            ffi::OsStrExt,
            fs::OpenOptionsExt,
            io::{AsRawHandle, FromRawHandle},
        },
        ptr,
    };
    use windows_sys::Win32::{
        Foundation::{LocalFree, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE},
        Security::{
            Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SECURITY_ATTRIBUTES,
        },
        Storage::FileSystem::*,
        System::SystemServices::FILE_NAMED_STREAMS,
    };
    fn wide(path: &Path) -> std::io::Result<Vec<u16>> {
        let mut s: Vec<u16> = path.as_os_str().encode_wide().collect();
        if s.contains(&0) || s.len() > 32_000 {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
        }
        s.push(0);
        Ok(s)
    }
    struct Descriptor(*mut core::ffi::c_void);
    impl Descriptor {
        fn new(directory: bool) -> std::io::Result<Self> {
            let sddl = if directory {
                "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;OW)"
            } else {
                "D:P(A;;FA;;;SY)(A;;FA;;;OW)"
            };
            let text: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
            let mut descriptor = ptr::null_mut();
            if unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    text.as_ptr(),
                    1,
                    &mut descriptor,
                    ptr::null_mut(),
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(Self(descriptor))
        }
        fn attributes(&self) -> SECURITY_ATTRIBUTES {
            SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: self.0,
                bInheritHandle: 0,
            }
        }
    }
    impl Drop for Descriptor {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
    pub fn create_private_file(path: &Path) -> std::io::Result<File> {
        let descriptor = Descriptor::new(false)?;
        let attributes = descriptor.attributes();
        let path = wide(path)?;
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE | DELETE,
                0,
                &attributes,
                CREATE_NEW,
                FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_TEMPORARY | FILE_FLAG_OPEN_REPARSE_POINT,
                ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_handle(handle) })
    }
    pub fn create_private_directory(path: &Path) -> std::io::Result<()> {
        let descriptor = Descriptor::new(true)?;
        let attributes = descriptor.attributes();
        if unsafe { CreateDirectoryW(wide(path)?.as_ptr(), &attributes) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    pub fn mark_internet_zone(path: &Path, file: &File) -> Result<()> {
        let mut flags = 0u32;
        let ok = unsafe {
            GetVolumeInformationByHandleW(
                file.as_raw_handle(),
                ptr::null_mut(),
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut flags,
                ptr::null_mut(),
                0,
            )
        };
        if ok == 0 || flags & FILE_NAMED_STREAMS == 0 {
            return Err(ToolError::new(
                "unsupported_filesystem",
                "This filesystem cannot store Windows Internet-zone metadata.",
            ));
        }
        let mut stream: OsString = path.as_os_str().to_owned();
        stream.push(":Zone.Identifier");
        let marker = b"[ZoneTransfer]\r\nZoneId=3\r\n";
        // Sharing is per stream. DELETE sharing is required because the primary
        // handle already has DELETE access; it remains exclusively open.
        let mut zone = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .share_mode(FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(stream)
            .map_err(|_| {
                ToolError::new(
                    "zone_metadata",
                    "Internet-zone metadata could not be applied.",
                )
            })?;
        let written = (|| -> std::io::Result<()> {
            zone.write_all(marker)?;
            zone.sync_all()?;
            zone.seek(SeekFrom::Start(0))?;
            let mut actual = Vec::new();
            zone.take(marker.len() as u64 + 1)
                .read_to_end(&mut actual)?;
            if actual != marker {
                return Err(std::io::Error::from(std::io::ErrorKind::InvalidData));
            }
            Ok(())
        })();
        written.map_err(|_| {
            ToolError::new(
                "zone_metadata",
                "Internet-zone metadata could not be verified.",
            )
        })
    }
    pub fn delete_on_close(file: &File) -> std::io::Result<()> {
        let information = FILE_DISPOSITION_INFO { DeleteFile: 1 };
        if unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FileDispositionInfo,
                &information as *const _ as _,
                size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    pub fn publish(file: &File, destination: &Path) -> std::io::Result<()> {
        // Rename the verified handle, preserving both the data and its zone
        // stream. ReplaceIfExists=false is enforced by the kernel atomically.
        let filename = wide(destination)?;
        let length = offset_of!(FILE_RENAME_INFO, FileName) + filename.len() * 2;
        let layout = std::alloc::Layout::from_size_align(
            length.max(size_of::<FILE_RENAME_INFO>()),
            align_of::<FILE_RENAME_INFO>(),
        )
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
        struct Buffer {
            data: *mut u8,
            layout: std::alloc::Layout,
        }
        impl Drop for Buffer {
            fn drop(&mut self) {
                unsafe {
                    std::alloc::dealloc(self.data, self.layout);
                }
            }
        }
        let data = unsafe { std::alloc::alloc_zeroed(layout) };
        if data.is_null() {
            return Err(std::io::Error::from(std::io::ErrorKind::OutOfMemory));
        }
        let buffer = Buffer { data, layout };
        let info = buffer.data as *mut FILE_RENAME_INFO;
        unsafe {
            (*info).Anonymous.ReplaceIfExists = 0;
            (*info).RootDirectory = ptr::null_mut();
            (*info).FileNameLength = ((filename.len() - 1) * 2) as u32;
            ptr::copy_nonoverlapping(
                filename.as_ptr(),
                (*info).FileName.as_mut_ptr(),
                filename.len(),
            );
            if SetFileInformationByHandle(
                file.as_raw_handle(),
                FileRenameInfo,
                info as _,
                length as u32,
            ) == 0
            {
                return Err(std::io::Error::last_os_error());
            }
            let attributes = FILE_BASIC_INFO {
                CreationTime: 0,
                LastAccessTime: 0,
                LastWriteTime: 0,
                ChangeTime: 0,
                FileAttributes: FILE_ATTRIBUTE_NORMAL,
            };
            if SetFileInformationByHandle(
                file.as_raw_handle(),
                FileBasicInfo,
                &attributes as *const _ as _,
                size_of::<FILE_BASIC_INFO>() as u32,
            ) == 0
            {
                // The caller still owns the handle and Drop deletes this exact
                // renamed file on error, before reporting any success.
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    const ID: &str = "0123456789abcdef0123456789abcdef";
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "openremote-upload-test-{:032x}",
                rand::random::<u128>()
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
        fn receive(&self) -> PathBuf {
            self.0.join("ReceivedFiles")
        }
        fn receiver(&self) -> Uploads {
            Uploads::new(self.receive(), Box::new(TestZone(None)))
        }
        fn entries(&self) -> usize {
            fs::read_dir(self.receive()).map(|d| d.count()).unwrap_or(0)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    struct TestZone(Option<Arc<AtomicUsize>>);
    impl ZoneMarker for TestZone {
        fn mark(&self, _: &Path, _: &File) -> Result<()> {
            if let Some(n) = &self.0 {
                n.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        }
    }
    struct FailedZone;
    impl ZoneMarker for FailedZone {
        fn mark(&self, _: &Path, _: &File) -> Result<()> {
            Err(ToolError::new("zone_metadata", "Unable to mark the file."))
        }
    }
    fn digest(data: &[u8]) -> String {
        format!("{:x}", Sha256::digest(data))
    }
    fn begin(u: &mut Uploads, data: &[u8]) {
        u.begin(
            ID.into(),
            "report.txt".into(),
            data.len() as u64,
            digest(data),
        )
        .unwrap();
    }
    #[test]
    fn valid_chunks_finish_with_exact_basename_length_digest_and_marker() {
        let f = Fixture::new();
        let n = Arc::new(AtomicUsize::new(0));
        let mut u = Uploads::new(f.receive(), Box::new(TestZone(Some(n.clone()))));
        let data = b"first\0second";
        begin(&mut u, data);
        assert_eq!(u.chunk(ID, 0, &data[..5]).unwrap()["received"], 5);
        assert_eq!(u.chunk(ID, 5, &data[5..]).unwrap()["received"], data.len());
        let result = u.finish(ID, || true).unwrap();
        u.commit_finished();
        let name = format!("{ID}_report.txt");
        assert_eq!(
            result,
            json!({"transferId":ID,"name":name,"size":data.len(),"sha256":digest(data)})
        );
        assert_eq!(fs::read(f.receive().join(name)).unwrap(), data);
        assert_eq!(n.load(Ordering::SeqCst), 1);
        assert_eq!(f.entries(), 1);
    }
    #[test]
    fn empty_upload_is_valid_and_duplicate_transfer_cannot_reopen_it() {
        let f = Fixture::new();
        let mut u = f.receiver();
        begin(&mut u, b"");
        u.finish(ID, || true).unwrap();
        u.commit_finished();
        assert_eq!(
            u.begin(ID.into(), "different.txt".into(), 0, digest(b""))
                .unwrap_err()
                .code,
            "duplicate_transfer"
        );
        assert_eq!(f.entries(), 1);
    }
    #[test]
    fn wrong_offset_removes_partial_and_cannot_resume() {
        let f = Fixture::new();
        let mut u = f.receiver();
        begin(&mut u, b"abc");
        u.chunk(ID, 0, b"a").unwrap();
        assert_eq!(u.chunk(ID, 0, b"a").unwrap_err().code, "offset_mismatch");
        assert_eq!(f.entries(), 0);
        assert!(u.chunk(ID, 1, b"bc").is_err());
    }
    #[test]
    fn oversized_chunk_length_overrun_and_short_finish_remove_partials() {
        for action in 0..3 {
            let f = Fixture::new();
            let mut u = f.receiver();
            begin(&mut u, b"abc");
            let r = match action {
                0 => u.chunk(ID, 0, &vec![0; MAX_CHUNK + 1]),
                1 => u.chunk(ID, 0, b"abcd"),
                _ => u.finish(ID, || true),
            };
            assert!(r.is_err());
            assert_eq!(f.entries(), 0);
        }
    }
    #[test]
    fn wrong_hash_and_failed_zone_metadata_never_publish_a_file() {
        let f = Fixture::new();
        let mut u = f.receiver();
        begin(&mut u, b"abc");
        u.chunk(ID, 0, b"bad").unwrap();
        assert_eq!(u.finish(ID, || true).unwrap_err().code, "hash_mismatch");
        assert_eq!(f.entries(), 0);
        let mut u = Uploads::new(f.receive(), Box::new(FailedZone));
        begin(&mut u, b"");
        assert_eq!(u.finish(ID, || true).unwrap_err().code, "zone_metadata");
        assert_eq!(f.entries(), 0);
    }
    #[test]
    fn publication_is_rolled_back_until_dispatcher_commits_authorization() {
        let f = Fixture::new();
        let mut u = f.receiver();
        begin(&mut u, b"");
        u.finish(ID, || true).unwrap();
        assert!(u.finished.is_some());
        u.cancel_all();
        assert_eq!(f.entries(), 0);
        let mut u = f.receiver();
        begin(&mut u, b"");
        u.finish(ID, || true).unwrap();
        u.commit_finished();
        u.cancel_all();
        assert_eq!(f.entries(), 1);
    }
    #[test]
    fn cancellation_drop_and_idle_expiry_delete_only_partial() {
        let f = Fixture::new();
        for action in 0..3 {
            let mut u = f.receiver();
            begin(&mut u, b"abc");
            assert_eq!(f.entries(), 1);
            if action == 0 {
                u.cancel(ID).unwrap();
            } else if action == 1 {
                u.expire(Instant::now() + IDLE);
            } else {
                drop(u);
            }
            assert_eq!(f.entries(), 0);
        }
    }
    #[test]
    fn quota_counts_cancelled_reservations_and_busy_cannot_replace_active() {
        let f = Fixture::new();
        let mut u = f.receiver();
        for n in 0..4 {
            let id = format!("{n:032x}");
            u.begin(id.clone(), "data.bin".into(), MAX_FILE, "a".repeat(64))
                .unwrap();
            assert_eq!(
                u.begin(format!("{:032x}", n + 10), "x".into(), 0, digest(b""))
                    .unwrap_err()
                    .code,
                "transfer_busy"
            );
            u.cancel(&id).unwrap();
        }
        assert_eq!(u.reserved, SESSION_QUOTA);
        assert_eq!(
            u.begin(ID.into(), "x".into(), 1, digest(b"x"))
                .unwrap_err()
                .code,
            "quota_exceeded"
        );
        assert_eq!(f.entries(), 0);
    }
    #[test]
    fn existing_destination_created_after_begin_is_never_overwritten() {
        let f = Fixture::new();
        let mut u = f.receiver();
        begin(&mut u, b"abc");
        u.chunk(ID, 0, b"abc").unwrap();
        let target = f.receive().join(format!("{ID}_report.txt"));
        fs::write(&target, b"keep me").unwrap();
        assert!(u.finish(ID, || true).is_err());
        assert_eq!(fs::read(target).unwrap(), b"keep me");
        assert_eq!(f.entries(), 1);
    }
    #[test]
    fn invalid_names_sizes_and_ids_do_not_create_receive_directory() {
        let f = Fixture::new();
        let mut u = f.receiver();
        assert!(u
            .begin(ID.into(), "../escape".into(), 0, digest(b""))
            .is_err());
        assert!(u
            .begin(ID.into(), "NUL.txt".into(), 0, digest(b""))
            .is_err());
        assert!(u
            .begin(ID.into(), "valid".into(), MAX_FILE + 1, digest(b""))
            .is_err());
        assert!(u
            .begin("../x".into(), "valid".into(), 0, digest(b""))
            .is_err());
        assert!(!f.receive().exists());
    }
    #[cfg(unix)]
    #[test]
    fn symlink_receive_directory_is_rejected() {
        let f = Fixture::new();
        let outside = f.0.join("outside");
        fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, f.receive()).unwrap();
        let mut u = f.receiver();
        assert_eq!(
            u.begin(ID.into(), "x".into(), 0, digest(b""))
                .unwrap_err()
                .code,
            "unsafe_directory"
        );
        assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
    }
    #[cfg(not(windows))]
    #[test]
    fn native_marker_fails_explicitly_off_windows() {
        let f = Fixture::new();
        let mut u = Uploads::new(f.receive(), Box::new(InternetZone));
        begin(&mut u, b"");
        assert_eq!(
            u.finish(ID, || true).unwrap_err().code,
            "unsupported_filesystem"
        );
        assert_eq!(f.entries(), 0);
    }
}
