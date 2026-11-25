pub mod api;
pub mod bundled_encoder;
pub mod config;
pub mod connections;
pub mod displays;
pub mod host_home;
pub mod identity;
pub mod input;
pub mod media;
pub mod pairing;
pub mod protocol;
pub mod session;
pub mod session_tools;
pub mod startup;
pub mod ui;

/// The host must keep its activation gate closed after this failure. An end
/// acknowledgement alone cannot establish that native capture/input has stopped.
#[derive(Debug)]
pub struct UnverifiedTeardown;
impl std::fmt::Display for UnverifiedTeardown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("session teardown could not be verified; restart the host")
    }
}
impl std::error::Error for UnverifiedTeardown {}
