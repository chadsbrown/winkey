//! WinKeyer protocol types: version, paddle mode, mode register, pin config.

use bitflags::bitflags;

/// Detected WinKeyer hardware version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinKeyerVersion {
    /// WinKeyer2 (version byte 20..=23)
    Wk2,
    /// WinKeyer3 (version byte 30)
    Wk3,
    /// WinKeyer3.1 (version byte 31)
    Wk31,
}

impl WinKeyerVersion {
    /// Detect version from the host-open response byte.
    pub fn from_version_byte(byte: u8) -> Option<Self> {
        match byte {
            20..=23 => Some(Self::Wk2),
            30 => Some(Self::Wk3),
            31 => Some(Self::Wk31),
            _ => None,
        }
    }

    /// Whether this version supports WK3 extended commands.
    pub fn supports_wk3(&self) -> bool {
        matches!(self, Self::Wk3 | Self::Wk31)
    }

    /// Raw version byte for display.
    pub fn version_byte(&self) -> u8 {
        match self {
            Self::Wk2 => 23,
            Self::Wk3 => 30,
            Self::Wk31 => 31,
        }
    }
}

/// Paddle keying mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaddleMode {
    /// Iambic A (self-completing, no dot/dash memory)
    IambicA,
    /// Iambic B (self-completing, with dot/dash memory)
    #[default]
    IambicB,
    /// Ultimatic (last paddle pressed wins)
    Ultimatic,
    /// Bug mode (automatic dots, manual dashes)
    Bug,
}

impl PaddleMode {
    /// Encode as the two mode-register bits (bits 5-4).
    pub fn to_mode_bits(self) -> u8 {
        match self {
            Self::IambicB => 0x00,
            Self::IambicA => 0x10,
            Self::Ultimatic => 0x20,
            Self::Bug => 0x30,
        }
    }
}

bitflags! {
    /// WinKeyer Mode Register (command 0x0E).
    ///
    /// Bit layout (per K1EL WK3 Datasheet v1.3, Table 12):
    /// - Bit 7: Disable paddle watchdog (set = watchdog OFF)
    /// - Bit 6: Paddle echo-back enable
    /// - Bits 5-4: Paddle mode (see PaddleMode)
    /// - Bit 3: Swap paddles
    /// - Bit 2: Serial echo-back enable
    /// - Bit 1: Auto-space
    /// - Bit 0: Contest spacing
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ModeRegister: u8 {
        const PADDLE_WATCHDOG_DISABLE = 0x80;
        const PADDLE_ECHO    = 0x40;
        const SWAP_PADDLES   = 0x08;
        const SERIAL_ECHO    = 0x04;
        const AUTO_SPACE     = 0x02;
        const CONTEST_SPACING = 0x01;
    }
}

impl Default for ModeRegister {
    fn default() -> Self {
        Self::PADDLE_ECHO | Self::SERIAL_ECHO
    }
}

impl ModeRegister {
    /// Combine mode register flags with a paddle mode to produce the full byte.
    pub fn with_paddle_mode(self, mode: PaddleMode) -> u8 {
        self.bits() | mode.to_mode_bits()
    }
}

bitflags! {
    /// WinKeyer Pin Configuration (command 0x09).
    ///
    /// Bit layout (per K1EL WK3 Datasheet v1.3, Table 10):
    /// - Bit 0: PTT enable
    /// - Bit 1: Sidetone enable
    /// - Bit 2: Key output enable (Pin 3 on WKUSB — primary CW output)
    /// - Bit 3: Key output 2 enable (Pin 5 on WKUSB — secondary output)
    /// - Bits 4-5: Hang time (2 bits)
    /// - Bits 6-7: Ultimatic priority (2 bits)
    ///
    /// NOTE: The datasheet Table 10 labels bit 2 as "KeyOut 2" and bit 3 as
    /// "KeyOut 1", but the WK1 table clarifies bit 2 = Pin 3 and bit 3 = Pin 5.
    /// On WKUSB, Pin 3 is the primary CW key output. We name bit 2 as
    /// KEY_OUTPUT for API clarity since it's the port most rigs connect to.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PinConfig: u8 {
        const PTT_ENABLE       = 0x01;
        const SIDETONE_ENABLE  = 0x02;
        const KEY_OUTPUT       = 0x04;
        const KEY_OUTPUT_2     = 0x08;
        const HANG_TIME_0      = 0x10;
        const HANG_TIME_1      = 0x20;
    }
}

impl Default for PinConfig {
    fn default() -> Self {
        // 0x07 = PTT + Sidetone + Key Output (Pin 3, primary CW output)
        Self::PTT_ENABLE | Self::SIDETONE_ENABLE | Self::KEY_OUTPUT
    }
}

/// Compute the sidetone control byte for a given frequency and version.
///
/// - WK2: values 1-10, frequency = 4000/N Hz (stepped)
/// - WK3: byte = 62500/freq, continuously adjustable 500-4000 Hz
pub fn sidetone_byte(freq_hz: u16, version: WinKeyerVersion) -> u8 {
    let freq_hz = freq_hz.clamp(500, 4000);
    if version.supports_wk3() {
        (62500u32 / freq_hz as u32) as u8
    } else {
        // WK2: map to nearest 1-10 value (4000/N)
        (4000u32 / freq_hz as u32).clamp(1, 10) as u8
    }
}

/// Parameters for the Load Defaults command (0x0F, 15 bytes).
///
/// Field order per K1EL WK3 Datasheet v1.3, Table 13.
#[derive(Debug, Clone)]
pub struct LoadDefaults {
    pub mode_register: u8,
    pub speed_wpm: u8,
    pub sidetone: u8,
    pub weight: u8,
    pub lead_in_time: u8,
    pub tail_time: u8,
    pub min_wpm: u8,
    pub wpm_range: u8,
    pub x2_mode: u8,
    pub key_compensation: u8,
    pub farnsworth_wpm: u8,
    pub paddle_setpoint: u8,
    pub dit_dah_ratio: u8,
    pub pin_config: u8,
    pub x1_mode: u8,
}

impl Default for LoadDefaults {
    fn default() -> Self {
        Self {
            mode_register: ModeRegister::default()
                .with_paddle_mode(PaddleMode::default()),
            speed_wpm: 20,
            sidetone: 5,          // ~800 Hz
            weight: 50,           // 50% (standard)
            lead_in_time: 0,
            tail_time: 0,
            min_wpm: 10,
            wpm_range: 25,        // 10-35 WPM pot range
            x2_mode: 0,          // no extended features
            key_compensation: 0,
            farnsworth_wpm: 0,    // 0 = disabled
            paddle_setpoint: 50,
            dit_dah_ratio: 50,    // 50 = 3:1 standard
            pin_config: PinConfig::default().bits(),
            x1_mode: 0,          // no extra letterspace
        }
    }
}

impl LoadDefaults {
    /// Encode as the 15-byte parameter block (without the 0x0F prefix).
    pub fn to_bytes(&self) -> [u8; 15] {
        [
            self.mode_register,
            self.speed_wpm,
            self.sidetone,
            self.weight,
            self.lead_in_time,
            self.tail_time,
            self.min_wpm,
            self.wpm_range,
            self.x2_mode,
            self.key_compensation,
            self.farnsworth_wpm,
            self.paddle_setpoint,
            self.dit_dah_ratio,
            self.pin_config,
            self.x1_mode,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_detection() {
        assert_eq!(WinKeyerVersion::from_version_byte(23), Some(WinKeyerVersion::Wk2));
        assert_eq!(WinKeyerVersion::from_version_byte(20), Some(WinKeyerVersion::Wk2));
        assert_eq!(WinKeyerVersion::from_version_byte(30), Some(WinKeyerVersion::Wk3));
        assert_eq!(WinKeyerVersion::from_version_byte(31), Some(WinKeyerVersion::Wk31));
        assert_eq!(WinKeyerVersion::from_version_byte(0), None);
        assert_eq!(WinKeyerVersion::from_version_byte(10), None);
        assert_eq!(WinKeyerVersion::from_version_byte(50), None);
    }

    #[test]
    fn wk3_support() {
        assert!(!WinKeyerVersion::Wk2.supports_wk3());
        assert!(WinKeyerVersion::Wk3.supports_wk3());
        assert!(WinKeyerVersion::Wk31.supports_wk3());
    }

    #[test]
    fn paddle_mode_bits() {
        assert_eq!(PaddleMode::IambicB.to_mode_bits(), 0x00);
        assert_eq!(PaddleMode::IambicA.to_mode_bits(), 0x10);
        assert_eq!(PaddleMode::Ultimatic.to_mode_bits(), 0x20);
        assert_eq!(PaddleMode::Bug.to_mode_bits(), 0x30);
    }

    #[test]
    fn mode_register_with_paddle() {
        let mode = ModeRegister::SERIAL_ECHO | ModeRegister::CONTEST_SPACING;
        let byte = mode.with_paddle_mode(PaddleMode::IambicA);
        // SERIAL_ECHO=0x04, CONTEST_SPACING=0x01, IambicA=0x10
        assert_eq!(byte, 0x04 | 0x01 | 0x10);
    }

    #[test]
    fn mode_register_default() {
        let mode = ModeRegister::default();
        assert!(mode.contains(ModeRegister::SERIAL_ECHO));
        assert!(mode.contains(ModeRegister::PADDLE_ECHO));
        // PADDLE_ECHO=0x40, SERIAL_ECHO=0x04 → 0x44
        assert_eq!(mode.bits(), 0x44);
    }

    #[test]
    fn pin_config_default() {
        let pin = PinConfig::default();
        assert!(pin.contains(PinConfig::PTT_ENABLE));
        assert!(pin.contains(PinConfig::SIDETONE_ENABLE));
        assert!(pin.contains(PinConfig::KEY_OUTPUT));
        // PTT=0x01, SIDETONE=0x02, KEY_OUTPUT=0x04 → 0x07
        assert_eq!(pin.bits(), 0x07);
    }

    #[test]
    fn load_defaults_encoding() {
        let defaults = LoadDefaults::default();
        let bytes = defaults.to_bytes();
        assert_eq!(bytes.len(), 15);
        assert_eq!(bytes[1], 20); // speed_wpm
        assert_eq!(bytes[6], 10); // min_wpm
    }

    #[test]
    fn load_defaults_roundtrip() {
        let mut d = LoadDefaults::default();
        d.speed_wpm = 28;
        d.lead_in_time = 4;
        d.tail_time = 3;
        let bytes = d.to_bytes();
        assert_eq!(bytes[1], 28);
        assert_eq!(bytes[4], 4);
        assert_eq!(bytes[5], 3);
    }
}
