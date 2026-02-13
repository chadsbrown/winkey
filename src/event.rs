//! Event types emitted by the keyer.

/// Current status of the keyer hardware, decoded from WinKeyer status bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyerStatus {
    pub xoff: bool,
    pub breakin: bool,
    pub busy: bool,
    pub keydown: bool,
    pub waiting: bool,
}

impl KeyerStatus {
    /// Decode a WinKeyer status byte (bits 7-5 = 0b110).
    ///
    /// Bit layout per K1EL WK3 Datasheet v1.3, Tables 14-15:
    ///   1 1 0 Wait Keydown Busy Breakin Xoff
    ///   7 6 5  4      3      2     1      0
    ///
    /// Note: Keydown (bit 3) is only valid in WK1 mode. In WK2/WK3 mode,
    /// bit 3 = 0 indicates a regular status byte (vs pushbutton status).
    pub fn from_status_byte(byte: u8) -> Self {
        Self {
            xoff: byte & 0x01 != 0,
            breakin: byte & 0x02 != 0,
            busy: byte & 0x04 != 0,
            keydown: byte & 0x08 != 0,
            waiting: byte & 0x10 != 0,
        }
    }
}

/// Events emitted by any keyer backend via broadcast channel.
#[derive(Debug, Clone)]
pub enum KeyerEvent {
    /// Keyer status bits changed.
    StatusChanged(KeyerStatus),

    /// Speed pot value changed (WPM).
    SpeedPotChanged { wpm: u8 },

    /// A character was sent (echo-back from WinKeyer).
    CharacterSent(char),

    /// Paddle break-in detected (breakin bit 0→1 transition).
    PaddleBreakIn,

    /// Connection to keyer hardware established.
    Connected,

    /// Connection to keyer hardware lost.
    Disconnected,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_status_idle() {
        let status = KeyerStatus::from_status_byte(0xC0);
        assert!(!status.xoff);
        assert!(!status.breakin);
        assert!(!status.busy);
        assert!(!status.keydown);
        assert!(!status.waiting);
    }

    #[test]
    fn decode_status_xoff() {
        // Bit 0 = XOFF
        let status = KeyerStatus::from_status_byte(0xC1);
        assert!(status.xoff);
        assert!(!status.breakin);
    }

    #[test]
    fn decode_status_breakin() {
        // Bit 1 = BREAKIN
        let status = KeyerStatus::from_status_byte(0xC2);
        assert!(!status.xoff);
        assert!(status.breakin);
    }

    #[test]
    fn decode_status_busy_keydown() {
        // Bit 2 = BUSY, Bit 3 = KEYDOWN
        let status = KeyerStatus::from_status_byte(0xCC);
        assert!(status.busy);
        assert!(status.keydown);
    }

    #[test]
    fn decode_status_waiting() {
        // Bit 4 = WAIT
        let status = KeyerStatus::from_status_byte(0xD0);
        assert!(status.waiting);
    }

    #[test]
    fn decode_status_all_bits() {
        // Bits 0-4 all set: 0xC0 | 0x1F = 0xDF
        let status = KeyerStatus::from_status_byte(0xDF);
        assert!(status.xoff);
        assert!(status.breakin);
        assert!(status.busy);
        assert!(status.keydown);
        assert!(status.waiting);
    }
}
