//! WinKeyerBuilder: fluent configuration and init handshake.

use std::sync::atomic::AtomicU8;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::broadcast;
use tracing::{debug, info};

use crate::error::{Error, Result};
use crate::event::KeyerEvent;
use crate::io::spawn_io_task;
use crate::keyer::{KeyerCapabilities, KeyerInfo};
use crate::protocol::types::{
    LoadDefaults, ModeRegister, PaddleMode, PinConfig, WinKeyerVersion,
};
use crate::transport;
use crate::winkeyer::WinKeyer;

/// Builder for creating and configuring a WinKeyer connection.
///
/// # Example
///
/// ```no_run
/// # use winkey::builder::WinKeyerBuilder;
/// # use winkey::PaddleMode;
/// # async fn example() -> winkey::Result<()> {
/// let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
///     .speed(28)
///     .paddle_mode(PaddleMode::IambicB)
///     .contest_spacing(true)
///     .ptt_lead_in_ms(40)
///     .ptt_tail_ms(30)
///     .build()
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct WinKeyerBuilder {
    port_path: String,
    speed_wpm: u8,
    paddle_mode: PaddleMode,
    mode_flags: ModeRegister,
    pin_config: PinConfig,
    sidetone: u8,
    weight: u8,
    ptt_lead_in: u8,
    ptt_tail: u8,
    min_wpm: u8,
    wpm_range: u8,
    farnsworth_wpm: u8,
    dit_dah_ratio: u8,
    prefer_wk3: bool,
}

impl WinKeyerBuilder {
    /// Create a new builder for the given serial port path.
    pub fn new(port_path: &str) -> Self {
        Self {
            port_path: port_path.to_string(),
            speed_wpm: 20,
            paddle_mode: PaddleMode::default(),
            mode_flags: ModeRegister::default(),
            pin_config: PinConfig::default(),
            sidetone: 5,
            weight: 50,
            ptt_lead_in: 0,
            ptt_tail: 0,
            min_wpm: 10,
            wpm_range: 25,
            farnsworth_wpm: 0,
            dit_dah_ratio: 50,
            prefer_wk3: true,
        }
    }

    /// Set the initial CW speed in WPM (5-99).
    pub fn speed(mut self, wpm: u8) -> Self {
        self.speed_wpm = wpm;
        self
    }

    /// Set the paddle mode.
    pub fn paddle_mode(mut self, mode: PaddleMode) -> Self {
        self.paddle_mode = mode;
        self
    }

    /// Enable or disable contest spacing.
    pub fn contest_spacing(mut self, enabled: bool) -> Self {
        if enabled {
            self.mode_flags |= ModeRegister::CONTEST_SPACING;
        } else {
            self.mode_flags -= ModeRegister::CONTEST_SPACING;
        }
        self
    }

    /// Enable or disable auto-space.
    pub fn auto_space(mut self, enabled: bool) -> Self {
        if enabled {
            self.mode_flags |= ModeRegister::AUTO_SPACE;
        } else {
            self.mode_flags -= ModeRegister::AUTO_SPACE;
        }
        self
    }

    /// Enable or disable paddle swap.
    pub fn swap_paddles(mut self, enabled: bool) -> Self {
        if enabled {
            self.mode_flags |= ModeRegister::SWAP_PADDLES;
        } else {
            self.mode_flags -= ModeRegister::SWAP_PADDLES;
        }
        self
    }

    /// Set sidetone frequency (1-10).
    pub fn sidetone(mut self, value: u8) -> Self {
        self.sidetone = value;
        self
    }

    /// Set keying weight (10-90, default 50).
    pub fn weight(mut self, value: u8) -> Self {
        self.weight = value;
        self
    }

    /// Set PTT lead-in time in milliseconds (converted to 10ms units).
    pub fn ptt_lead_in_ms(mut self, ms: u16) -> Self {
        self.ptt_lead_in = (ms / 10).min(250) as u8;
        self
    }

    /// Set PTT tail time in milliseconds (converted to 10ms units).
    pub fn ptt_tail_ms(mut self, ms: u16) -> Self {
        self.ptt_tail = (ms / 10).min(250) as u8;
        self
    }

    /// Set minimum WPM for speed pot range.
    pub fn min_wpm(mut self, wpm: u8) -> Self {
        self.min_wpm = wpm;
        self
    }

    /// Set WPM range for speed pot.
    pub fn wpm_range(mut self, range: u8) -> Self {
        self.wpm_range = range;
        self
    }

    /// Set Farnsworth speed (0 = disable).
    pub fn farnsworth(mut self, wpm: u8) -> Self {
        self.farnsworth_wpm = wpm;
        self
    }

    /// Set dit/dah ratio (33-66, default 50 = 3:1).
    pub fn dit_dah_ratio(mut self, ratio: u8) -> Self {
        self.dit_dah_ratio = ratio;
        self
    }

    /// Set pin configuration.
    pub fn pin_config(mut self, config: PinConfig) -> Self {
        self.pin_config = config;
        self
    }

    /// Whether to prefer WK3 mode if hardware supports it (default true).
    pub fn prefer_wk3(mut self, enabled: bool) -> Self {
        self.prefer_wk3 = enabled;
        self
    }

    /// Build the WinKeyer connection using a real serial port.
    pub async fn build(self) -> Result<WinKeyer> {
        let port = transport::open_serial(&self.port_path, 1200)?;
        self.build_with_port(port).await
    }

    /// Build using a pre-opened port (for testing with MockPort).
    pub async fn build_with_port<P>(self, mut port: P) -> Result<WinKeyer>
    where
        P: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        // Step 1: Defensive close + wait
        debug!("sending defensive host close");
        port.write_all(&[0x00, 0x03]).await.map_err(|e| {
            Error::Transport(format!("failed to send defensive close: {e}"))
        })?;
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Drain any leftover bytes
        let mut drain_buf = [0u8; 64];
        loop {
            match tokio::time::timeout(Duration::from_millis(50), port.read(&mut drain_buf)).await {
                Ok(Ok(n)) if n > 0 => continue, // keep draining
                _ => break,
            }
        }

        // Step 2: Host Open
        debug!("sending host open");
        port.write_all(&[0x00, 0x02]).await.map_err(|e| {
            Error::Transport(format!("failed to send host open: {e}"))
        })?;

        // Step 3: Wait for version byte
        let mut version_buf = [0u8; 1];
        match tokio::time::timeout(Duration::from_secs(1), port.read_exact(&mut version_buf)).await
        {
            Ok(Ok(_n)) => {}
            Ok(Err(e)) => {
                return Err(Error::Transport(format!(
                    "failed to read version byte: {e}"
                )));
            }
            Err(_) => {
                return Err(Error::Timeout);
            }
        }

        let version_byte = version_buf[0];
        let version = WinKeyerVersion::from_version_byte(version_byte).ok_or_else(|| {
            Error::Protocol(format!(
                "unsupported WinKeyer version byte: {version_byte}"
            ))
        })?;

        info!(
            version = version_byte,
            wk3 = version.supports_wk3(),
            "WinKeyer detected"
        );

        // Step 4: Set WK2/WK3 mode
        if version.supports_wk3() && self.prefer_wk3 {
            debug!("setting WK3 mode");
            port.write_all(&[0x00, 0x13]).await.map_err(|e| {
                Error::Transport(format!("failed to set WK3 mode: {e}"))
            })?;
        } else {
            debug!("setting WK2 mode");
            port.write_all(&[0x00, 0x0B]).await.map_err(|e| {
                Error::Transport(format!("failed to set WK2 mode: {e}"))
            })?;
        }

        // Step 5: Load Defaults
        let defaults = LoadDefaults {
            mode_register: self.mode_flags.with_paddle_mode(self.paddle_mode),
            speed_wpm: self.speed_wpm,
            sidetone: self.sidetone,
            weight: self.weight,
            lead_in_time: self.ptt_lead_in,
            tail_time: self.ptt_tail,
            min_wpm: self.min_wpm,
            wpm_range: self.wpm_range,
            extension: 0,
            key_compensation: 0,
            farnsworth_wpm: self.farnsworth_wpm,
            paddle_setpoint: 50,
            dit_dah_ratio: self.dit_dah_ratio,
            pin_config: self.pin_config.bits(),
            pot_range_low: self.min_wpm,
        };

        let cmd = crate::protocol::command::load_defaults(&defaults);
        debug!("loading defaults: {:02X?}", cmd);
        port.write_all(&cmd).await.map_err(|e| {
            Error::Transport(format!("failed to load defaults: {e}"))
        })?;

        // Step 6: Clear buffer + drain post-init status bytes.
        // The WK may send status bytes (sometimes with XOFF set) after
        // loading defaults. Clear the buffer to reset the WK state and
        // drain any pending responses before the IO task starts.
        port.write_all(&[0x0A]).await.map_err(|e| {
            Error::Transport(format!("failed to send clear buffer: {e}"))
        })?;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Drain post-init bytes
        let mut drain_buf = [0u8; 64];
        loop {
            match tokio::time::timeout(Duration::from_millis(50), port.read(&mut drain_buf)).await {
                Ok(Ok(n)) if n > 0 => {
                    debug!("drained {} post-init bytes: {:02X?}", n, &drain_buf[..n]);
                    continue;
                }
                _ => break,
            }
        }

        // Step 7: Re-assert mode register to ensure serial echo is enabled.
        // Some WK3.1 firmware may not apply the mode register from LoadDefaults
        // reliably, so set it explicitly.
        let mode_byte = defaults.mode_register;
        debug!("setting mode register: 0x{mode_byte:02X}");
        port.write_all(&[0x0E, mode_byte]).await.map_err(|e| {
            Error::Transport(format!("failed to set mode register: {e}"))
        })?;

        // Step 8: Spawn IO task
        let (event_tx, _) = broadcast::channel::<KeyerEvent>(256);
        let _ = event_tx.send(KeyerEvent::Connected);

        let io = spawn_io_task(port, event_tx.clone(), self.min_wpm);

        let version_str = format!(
            "WinKeyer {} (v{})",
            match version {
                WinKeyerVersion::Wk2 => "2",
                WinKeyerVersion::Wk3 => "3",
                WinKeyerVersion::Wk31 => "3.1",
            },
            version_byte
        );

        Ok(WinKeyer {
            io,
            info: KeyerInfo {
                name: version_str,
                version: format!("{}", version_byte),
                port: Some(self.port_path),
            },
            capabilities: KeyerCapabilities {
                speed_pot: true,
                sidetone: true,
                ptt_control: true,
                paddle_echo: true,
                prosigns: true,
                buffered_speed: true,
                farnsworth: true,
                contest_spacing: true,
            },
            version,
            event_tx,
            speed: AtomicU8::new(self.speed_wpm),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyer::Keyer;
    use crate::transport::MockPort;

    /// Create a mock and schedule the version byte to arrive after the drain
    /// window (the builder drains for ~150ms, so queue after 200ms).
    fn mock_with_delayed_version(version_byte: u8) -> MockPort {
        let mock = MockPort::new();
        let mock_clone = mock.clone();
        tokio::spawn(async move {
            // Wait for the defensive close + drain to complete
            tokio::time::sleep(Duration::from_millis(200)).await;
            mock_clone.queue_read(&[version_byte]);
        });
        mock
    }

    #[tokio::test]
    async fn build_with_wk2() {
        let mock = mock_with_delayed_version(23);
        let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
            .speed(28)
            .build_with_port(mock.clone())
            .await
            .unwrap();

        assert_eq!(keyer.version(), WinKeyerVersion::Wk2);
        assert_eq!(keyer.info().version, "23");

        // Verify handshake: defensive close + host open + wk2 mode + load defaults
        let written = mock.written_data();
        // Defensive close
        assert_eq!(&written[0..2], &[0x00, 0x03]);
        // Host open
        assert_eq!(&written[2..4], &[0x00, 0x02]);
        // WK2 mode (not WK3 since version is WK2)
        assert_eq!(&written[4..6], &[0x00, 0x0B]);
        // Load defaults (0x0F + 15 bytes)
        assert_eq!(written[6], 0x0F);
        // Clear buffer (0x0A) after load defaults
        assert_eq!(written[22], 0x0A);
        // Mode register re-assert (0x0E + mode byte)
        assert_eq!(written[23], 0x0E);
        assert_eq!(written[24], 0xC0); // SERIAL_ECHO | PADDLE_ECHO
        assert_eq!(written.len(), 6 + 16 + 1 + 2); // 6 prefix + 16 defaults + 1 clear + 2 mode

        keyer.close().await.unwrap();
    }

    #[tokio::test]
    async fn build_with_wk3() {
        let mock = mock_with_delayed_version(30);
        let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
            .build_with_port(mock.clone())
            .await
            .unwrap();

        assert_eq!(keyer.version(), WinKeyerVersion::Wk3);

        // Should set WK3 mode
        let written = mock.written_data();
        assert_eq!(&written[4..6], &[0x00, 0x13]); // WK3 mode

        keyer.close().await.unwrap();
    }

    #[tokio::test]
    async fn build_with_wk3_prefer_wk2() {
        let mock = mock_with_delayed_version(30);
        let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
            .prefer_wk3(false)
            .build_with_port(mock.clone())
            .await
            .unwrap();

        // Should set WK2 mode even though hardware is WK3
        let written = mock.written_data();
        assert_eq!(&written[4..6], &[0x00, 0x0B]); // WK2 mode

        keyer.close().await.unwrap();
    }

    #[tokio::test]
    async fn build_with_invalid_version() {
        let mock = mock_with_delayed_version(10);
        let result = WinKeyerBuilder::new("/dev/ttyUSB0")
            .build_with_port(mock)
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::Protocol(_)));
    }

    #[tokio::test]
    async fn build_contest_spacing() {
        let mock = mock_with_delayed_version(23);
        let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
            .contest_spacing(true)
            .build_with_port(mock.clone())
            .await
            .unwrap();

        // Verify the mode register byte includes contest spacing
        let written = mock.written_data();
        let mode_byte = written[7]; // First byte of LoadDefaults params
        assert!(mode_byte & 0x02 != 0, "contest spacing bit should be set");

        keyer.close().await.unwrap();
    }

    #[tokio::test]
    async fn build_speed_setting() {
        let mock = mock_with_delayed_version(23);
        let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
            .speed(35)
            .build_with_port(mock.clone())
            .await
            .unwrap();

        // Verify speed in load defaults
        let written = mock.written_data();
        let speed_byte = written[8]; // Second byte of LoadDefaults params
        assert_eq!(speed_byte, 35);

        // Get speed should return builder value
        assert_eq!(keyer.get_speed().await.unwrap(), 35);

        keyer.close().await.unwrap();
    }

    #[tokio::test]
    async fn build_ptt_timing() {
        let mock = mock_with_delayed_version(23);
        let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
            .ptt_lead_in_ms(40) // 4 units
            .ptt_tail_ms(30)    // 3 units
            .build_with_port(mock.clone())
            .await
            .unwrap();

        let written = mock.written_data();
        let lead_in = written[11]; // 5th byte of LoadDefaults params
        let tail = written[12];    // 6th byte
        assert_eq!(lead_in, 4);
        assert_eq!(tail, 3);

        keyer.close().await.unwrap();
    }

    #[tokio::test]
    async fn keyer_trait_object_safety() {
        let mock = mock_with_delayed_version(23);
        let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
            .build_with_port(mock)
            .await
            .unwrap();

        // Verify Keyer trait is object-safe
        let _: Box<dyn Keyer> = Box::new(keyer);
    }
}
