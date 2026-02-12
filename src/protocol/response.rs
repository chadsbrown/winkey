//! Response parsing: WinKeyer → host.
//!
//! All functions are pure (no I/O). They classify and decode bytes
//! received from the WinKeyer.

use crate::event::KeyerStatus;

/// Classification of a received byte from WinKeyer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseByte {
    /// Status byte (bits 7-6 = 0b11). Contains keyer status flags.
    Status(KeyerStatus),

    /// Speed pot change (bits 7-6 = 0b10). Value is the pot reading.
    SpeedPot { value: u8 },

    /// Echo byte (bit 7 = 0). The character just sent by WinKeyer.
    Echo(char),
}

/// Classify a single byte received from WinKeyer.
///
/// WinKeyer uses bits 7-6 to distinguish response types:
/// - `0b11` (0xC0-0xFF): Status byte
/// - `0b10` (0x80-0xBF): Speed pot value
/// - `0b0x` (0x00-0x7F): Echo-back character
pub fn classify_byte(byte: u8) -> ResponseByte {
    match byte & 0xC0 {
        0xC0 => ResponseByte::Status(KeyerStatus::from_status_byte(byte)),
        0x80 => ResponseByte::SpeedPot {
            value: byte & 0x3F,
        },
        _ => ResponseByte::Echo(byte as char),
    }
}

/// Decode a status byte into KeyerStatus.
pub fn decode_status(byte: u8) -> KeyerStatus {
    KeyerStatus::from_status_byte(byte)
}

/// Decode a speed pot byte into WPM.
///
/// The pot value (0-31) is added to `min_wpm` to get the actual speed.
pub fn decode_speed_pot(byte: u8, min_wpm: u8) -> u8 {
    let pot_value = byte & 0x3F;
    min_wpm.saturating_add(pot_value)
}

/// Decode an echo byte into a character.
pub fn decode_echo(byte: u8) -> char {
    byte as char
}

/// Decode the version byte returned by Host Open command.
pub fn decode_version(byte: u8) -> Option<crate::protocol::types::WinKeyerVersion> {
    crate::protocol::types::WinKeyerVersion::from_version_byte(byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_status_bytes() {
        // 0xC0 = status, all clear
        let r = classify_byte(0xC0);
        assert!(matches!(r, ResponseByte::Status(s) if !s.xoff && !s.busy));

        // 0xCA = status with xoff + busy
        let r = classify_byte(0xCA);
        assert!(
            matches!(r, ResponseByte::Status(s) if s.xoff && s.busy)
        );

        // 0xFF = status with all bits
        let r = classify_byte(0xFF);
        assert!(matches!(r, ResponseByte::Status(s) if s.xoff && s.breakin && s.busy && s.keydown && s.waiting));
    }

    #[test]
    fn classify_speed_pot_bytes() {
        // 0x80 = speed pot, value 0
        assert_eq!(classify_byte(0x80), ResponseByte::SpeedPot { value: 0 });

        // 0x8F = speed pot, value 15
        assert_eq!(classify_byte(0x8F), ResponseByte::SpeedPot { value: 15 });

        // 0x9F = speed pot, value 31
        assert_eq!(classify_byte(0x9F), ResponseByte::SpeedPot { value: 31 });

        // 0xBF = speed pot, value 63 (max in range)
        assert_eq!(classify_byte(0xBF), ResponseByte::SpeedPot { value: 63 });
    }

    #[test]
    fn classify_echo_bytes() {
        assert_eq!(classify_byte(b'A'), ResponseByte::Echo('A'));
        assert_eq!(classify_byte(b'5'), ResponseByte::Echo('5'));
        assert_eq!(classify_byte(b' '), ResponseByte::Echo(' '));
        assert_eq!(classify_byte(0x00), ResponseByte::Echo('\0'));
    }

    #[test]
    fn speed_pot_wpm_calculation() {
        assert_eq!(decode_speed_pot(0x80, 10), 10); // pot=0, min=10
        assert_eq!(decode_speed_pot(0x8A, 10), 20); // pot=10, min=10
        assert_eq!(decode_speed_pot(0x99, 5), 30);  // pot=25, min=5
    }

    #[test]
    fn speed_pot_saturating() {
        // Extreme values shouldn't overflow
        assert_eq!(decode_speed_pot(0xBF, 250), 255); // pot=63, min=250 → saturates
    }

    #[test]
    fn echo_decode() {
        assert_eq!(decode_echo(b'C'), 'C');
        assert_eq!(decode_echo(b'Q'), 'Q');
        assert_eq!(decode_echo(b' '), ' ');
    }

    #[test]
    fn version_decode() {
        use crate::protocol::types::WinKeyerVersion;
        assert_eq!(decode_version(23), Some(WinKeyerVersion::Wk2));
        assert_eq!(decode_version(30), Some(WinKeyerVersion::Wk3));
        assert_eq!(decode_version(31), Some(WinKeyerVersion::Wk31));
        assert_eq!(decode_version(0), None);
        assert_eq!(decode_version(15), None);
    }
}
