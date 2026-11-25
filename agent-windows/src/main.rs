#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
use anyhow::bail;
use anyhow::Result;

#[cfg(not(windows))]
fn main() -> Result<()> {
    bail!("The desktop application runs on Windows; use cargo test or examples/synthetic_peer for Linux verification");
}

#[cfg(windows)]
fn main() {
    // Register this before runtime, directory or display initialization. Panic
    // payloads can contain secrets and must never be shown or logged.
    std::panic::set_hook(Box::new(|_| {
        openremote_agent::host_home::revoke_for_panic();
        openremote_agent::startup::write_diagnostic("unexpected_failure");
        openremote_agent::startup::report_fatal("OpenRemote stopped unexpectedly. Close and reopen the app. If this continues, contact the app provider.");
    }));
    let result = std::panic::catch_unwind(|| -> Result<()> {
        anyhow::ensure!(std::env::var_os("LOCALAPPDATA").is_some(), "Windows did not provide LOCALAPPDATA. Launch OpenRemote from your signed-in desktop account.");
        let mode = openremote_agent::startup::LaunchMode::parse(std::env::args().skip(1))?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|_| {
                anyhow::anyhow!(
                    "OpenRemote could not start its background runtime. Restart the application."
                )
            })?;
        runtime.block_on(run(mode))
    });
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            openremote_agent::startup::report_fatal(&error.to_string());
            std::process::exit(1);
        }
        Err(_) => std::process::exit(101),
    }
}

#[cfg(windows)]
async fn run(mode: openremote_agent::startup::LaunchMode) -> Result<()> {
    std::fs::create_dir_all(openremote_agent::config::data_dir())
        .map_err(|_| anyhow::anyhow!("OpenRemote could not create its settings folder. Check your Windows account's access to LocalAppData."))?;
    let _instance = acquire_instance_lock()?;
    openremote_agent::displays::initialize_dpi_awareness()?;
    openremote_agent::startup::run(mode).await
}

#[cfg(windows)]
fn acquire_instance_lock() -> Result<std::fs::File> {
    use anyhow::Context;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT,
    };
    let file=std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false)
        .share_mode(0).custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(openremote_agent::config::data_dir().join("host-instance.lock"))
        .context("OpenRemote is already running or an installation is in progress for this Windows account. Close the existing host or installer before opening another. If neither is running, check access to LocalAppData\\OpenRemote.")?;
    anyhow::ensure!(
        file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
        "invalid host instance lock"
    );
    Ok(file)
}
