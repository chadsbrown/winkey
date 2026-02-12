//! Backend-agnostic keyer trait.
//!
//! Contest loggers program against `dyn Keyer` to support multiple
//! keyer backends (WinKeyer, cwdaemon, rig-internal keyer).

use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::error::Result;
use crate::event::KeyerEvent;

/// Metadata about a keyer backend.
#[derive(Debug, Clone)]
pub struct KeyerInfo {
    pub name: String,
    pub version: String,
    pub port: Option<String>,
}

/// Capability flags for a keyer backend.
#[derive(Debug, Clone, Default)]
pub struct KeyerCapabilities {
    pub speed_pot: bool,
    pub sidetone: bool,
    pub ptt_control: bool,
    pub paddle_echo: bool,
    pub prosigns: bool,
    pub buffered_speed: bool,
    pub farnsworth: bool,
    pub contest_spacing: bool,
}

/// Backend-agnostic keyer interface.
///
/// No WinKeyer-specific types appear in this trait. Contest loggers program
/// against `dyn Keyer`.
#[async_trait]
pub trait Keyer: Send + Sync {
    /// Keyer metadata (name, version, port).
    fn info(&self) -> &KeyerInfo;

    /// Capability flags.
    fn capabilities(&self) -> &KeyerCapabilities;

    /// Queue a CW message for sending. Blocks if XOFF is active.
    async fn send_message(&self, text: &str) -> Result<()>;

    /// Immediately abort any in-progress message and clear the buffer.
    async fn abort(&self) -> Result<()>;

    /// Set the CW speed in WPM (immediate, not buffered).
    async fn set_speed(&self, wpm: u8) -> Result<()>;

    /// Get the current CW speed in WPM.
    async fn get_speed(&self) -> Result<u8>;

    /// Enable or disable key-down tune mode.
    async fn set_tune(&self, on: bool) -> Result<()>;

    /// Enable or disable PTT output.
    async fn set_ptt(&self, on: bool) -> Result<()>;

    /// Subscribe to keyer events (status changes, echo, speed pot, etc.).
    fn subscribe(&self) -> broadcast::Receiver<KeyerEvent>;

    /// Close the connection and shut down the IO task.
    async fn close(&self) -> Result<()>;
}
