//! WinKeyer struct: implements the `Keyer` trait and exposes WK-specific methods.

use std::sync::atomic::Ordering;

use async_trait::async_trait;
use tokio::sync::broadcast;
use tracing::debug;

use crate::error::{Error, Result};
use crate::event::KeyerEvent;
use crate::io::IoHandle;
use crate::keyer::{Keyer, KeyerCapabilities, KeyerInfo};
use crate::protocol::{command, types::WinKeyerVersion};

/// WinKeyer hardware handle.
///
/// Implements the [`Keyer`] trait for backend-agnostic CW keying, and also
/// exposes WinKeyer-specific methods (prosigns, buffered speed, pointer
/// commands, etc.) directly.
pub struct WinKeyer {
    pub(crate) io: IoHandle,
    pub(crate) info: KeyerInfo,
    pub(crate) capabilities: KeyerCapabilities,
    pub(crate) version: WinKeyerVersion,
    pub(crate) event_tx: broadcast::Sender<KeyerEvent>,
    pub(crate) speed: std::sync::atomic::AtomicU8,
}


impl std::fmt::Debug for WinKeyer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WinKeyer")
            .field("info", &self.info)
            .field("version", &self.version)
            .field("speed", &self.speed.load(Ordering::Relaxed))
            .finish()
    }
}

impl WinKeyer {
    /// The detected WinKeyer hardware version.
    pub fn version(&self) -> WinKeyerVersion {
        self.version
    }

    // ------------------------------------------------------------------
    // WK-specific methods (not in Keyer trait)
    // ------------------------------------------------------------------

    /// Send a prosign (merged letters) via the buffer.
    pub async fn send_prosign(&self, c1: u8, c2: u8) -> Result<()> {
        self.wait_xoff().await?;
        let cmd = command::buffered_merge(c1, c2);
        self.io.bg_command(cmd.to_vec()).await
    }

    /// Set buffered speed change (takes effect in-buffer).
    pub async fn set_buffered_speed(&self, wpm: u8) -> Result<()> {
        self.wait_xoff().await?;
        let cmd = command::buffered_speed_change(wpm);
        self.io.bg_command(cmd.to_vec()).await
    }

    /// Cancel buffered speed change (restore original speed).
    pub async fn cancel_buffered_speed(&self) -> Result<()> {
        self.wait_xoff().await?;
        let cmd = command::cancel_buffered_speed();
        self.io.bg_command(cmd.to_vec()).await
    }

    /// Set keying weight (10-90, default 50).
    pub async fn set_weight(&self, weight: u8) -> Result<()> {
        if !(10..=90).contains(&weight) {
            return Err(Error::InvalidParameter(format!(
                "weight must be 10-90, got {weight}"
            )));
        }
        let cmd = command::set_weight(weight);
        self.io.rt_command(cmd.to_vec()).await
    }

    /// Set dit/dah ratio (33-66, default 50 = 3:1).
    pub async fn set_ratio(&self, ratio: u8) -> Result<()> {
        if !(33..=66).contains(&ratio) {
            return Err(Error::InvalidParameter(format!(
                "ratio must be 33-66, got {ratio}"
            )));
        }
        let cmd = command::set_ratio(ratio);
        self.io.rt_command(cmd.to_vec()).await
    }

    /// Set Farnsworth speed (0 = disable).
    pub async fn set_farnsworth(&self, wpm: u8) -> Result<()> {
        let cmd = command::set_farnsworth(wpm);
        self.io.rt_command(cmd.to_vec()).await
    }

    /// Set paddle mode (IambicA, IambicB, Ultimatic, Bug).
    pub async fn set_paddle_mode(&self, mode: crate::PaddleMode) -> Result<()> {
        // Read current mode register, update paddle bits
        let mode_byte = crate::ModeRegister::default().with_paddle_mode(mode);
        let cmd = command::set_mode_register(mode_byte);
        self.io.rt_command(cmd.to_vec()).await
    }

    /// Set sidetone frequency (1-10).
    pub async fn set_sidetone(&self, value: u8) -> Result<()> {
        if !(1..=10).contains(&value) {
            return Err(Error::InvalidParameter(format!(
                "sidetone must be 1-10, got {value}"
            )));
        }
        let cmd = command::sidetone_control(value);
        self.io.rt_command(cmd.to_vec()).await
    }

    /// Set PTT lead-in and tail times (in 10ms units).
    pub async fn set_ptt_timing(&self, lead_in: u8, tail: u8) -> Result<()> {
        let cmd = command::set_ptt_timing(lead_in, tail);
        self.io.rt_command(cmd.to_vec()).await
    }

    /// Pause or resume CW output.
    pub async fn set_pause(&self, paused: bool) -> Result<()> {
        let cmd = command::set_pause(paused);
        self.io.rt_command(cmd.to_vec()).await
    }

    /// Insert a timed wait into the buffer (seconds).
    pub async fn buffered_wait(&self, seconds: u8) -> Result<()> {
        self.wait_xoff().await?;
        let cmd = command::buffered_wait(seconds);
        self.io.bg_command(cmd.to_vec()).await
    }

    /// Pointer command for live callsign editing.
    pub async fn pointer_command(&self, subcmd: u8, data: &[u8]) -> Result<()> {
        let cmd = command::pointer_command(subcmd, data);
        self.io.bg_command(cmd).await
    }

    /// Simulate paddle input via software.
    pub async fn software_paddle(&self, dit: bool, dah: bool) -> Result<()> {
        let cmd = command::software_paddle(dit, dah);
        self.io.rt_command(cmd.to_vec()).await
    }

    /// Echo test: send byte, expect it back.
    pub async fn echo_test(&self, byte: u8) -> Result<u8> {
        let cmd = command::admin_echo_test(byte);
        let response = self.io.rt_command_read(cmd.to_vec(), 1).await?;
        Ok(response[0])
    }

    /// Load defaults (15-parameter block).
    pub async fn load_defaults(&self, defaults: &crate::LoadDefaults) -> Result<()> {
        let cmd = command::load_defaults(defaults);
        self.io.rt_command(cmd.to_vec()).await
    }

    /// Write raw bytes via the background (buffered) channel.
    pub async fn raw_write(&self, data: &[u8]) -> Result<()> {
        self.wait_xoff().await?;
        self.io.bg_command(data.to_vec()).await
    }

    /// Write raw bytes via the real-time (priority) channel.
    pub async fn raw_write_rt(&self, data: &[u8]) -> Result<()> {
        self.io.rt_command(data.to_vec()).await
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    /// Wait for XOFF to clear, with a timeout.
    async fn wait_xoff(&self) -> Result<()> {
        if !self.io.xoff.load(Ordering::Acquire) {
            return Ok(());
        }

        debug!("XOFF active, waiting for buffer space...");
        let mut rx = self.event_tx.subscribe();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);

        loop {
            if !self.io.xoff.load(Ordering::Acquire) {
                return Ok(());
            }
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Ok(KeyerEvent::StatusChanged(status))) if !status.xoff => {
                    return Ok(());
                }
                Ok(Ok(_)) => continue,
                Ok(Err(_)) => return Err(Error::NotConnected),
                Err(_) => return Err(Error::BufferFull),
            }
        }
    }
}

#[async_trait]
impl Keyer for WinKeyer {
    fn info(&self) -> &KeyerInfo {
        &self.info
    }

    fn capabilities(&self) -> &KeyerCapabilities {
        &self.capabilities
    }

    async fn send_message(&self, text: &str) -> Result<()> {
        command::validate_cw_text(text).map_err(Error::InvalidParameter)?;
        self.wait_xoff().await?;
        let bytes = command::encode_text(text);
        self.io.bg_command(bytes).await
    }

    async fn abort(&self) -> Result<()> {
        let cmd = command::clear_buffer();
        self.io.rt_command(cmd.to_vec()).await
    }

    async fn set_speed(&self, wpm: u8) -> Result<()> {
        if !(5..=99).contains(&wpm) {
            return Err(Error::InvalidParameter(format!(
                "speed must be 5-99 WPM, got {wpm}"
            )));
        }
        let cmd = command::set_speed(wpm);
        self.io.rt_command(cmd.to_vec()).await?;
        self.speed.store(wpm, Ordering::Release);
        Ok(())
    }

    async fn get_speed(&self) -> Result<u8> {
        Ok(self.speed.load(Ordering::Acquire))
    }

    async fn set_tune(&self, on: bool) -> Result<()> {
        let cmd = command::key_immediate(on);
        self.io.rt_command(cmd.to_vec()).await
    }

    async fn set_ptt(&self, on: bool) -> Result<()> {
        // Use buffered PTT for sequenced operation
        let cmd = command::buffered_ptt(on);
        self.io.rt_command(cmd.to_vec()).await
    }

    fn subscribe(&self) -> broadcast::Receiver<KeyerEvent> {
        self.event_tx.subscribe()
    }

    async fn close(&self) -> Result<()> {
        // Send host close command before shutting down
        let cmd = command::admin_host_close();
        let _ = self.io.rt_command(cmd.to_vec()).await;
        self.io.shutdown().await
    }
}

impl Drop for WinKeyer {
    fn drop(&mut self) {
        self.io.cancel.cancel();
        self.io.task.abort();
    }
}
