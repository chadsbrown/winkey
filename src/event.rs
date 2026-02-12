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
    /// Decode a WinKeyer status byte (bits 7-6 = 0b11).
    /// Bit layout: 1 1 Wait Keydown Busy Breakin Xoff 0
    pub fn from_status_byte(byte: u8) -> Self {
        Self {
            xoff: byte & 0x02 != 0,
            breakin: byte & 0x04 != 0,
            busy: byte & 0x08 != 0,
            keydown: byte & 0x10 != 0,
            waiting: byte & 0x20 != 0,
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
        let status = KeyerStatus::from_status_byte(0xC2);
        assert!(status.xoff);
        assert!(!status.breakin);
    }

    #[test]
    fn decode_status_breakin() {
        let status = KeyerStatus::from_status_byte(0xC4);
        assert!(!status.xoff);
        assert!(status.breakin);
    }

    #[test]
    fn decode_status_busy_keydown() {
        let status = KeyerStatus::from_status_byte(0xD8);
        assert!(status.busy);
        assert!(status.keydown);
    }

    #[test]
    fn decode_status_waiting() {
        let status = KeyerStatus::from_status_byte(0xE0);
        assert!(status.waiting);
    }

    #[test]
    fn decode_status_all_bits() {
        let status = KeyerStatus::from_status_byte(0xFE);
        assert!(status.xoff);
        assert!(status.breakin);
        assert!(status.busy);
        assert!(status.keydown);
        assert!(status.waiting);
    }
}
