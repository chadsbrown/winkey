//! Prosign constants and contest message builder.
//!
//! Provides helpers for building CW messages with inline prosigns
//! and speed changes, encoding them into WinKeyer command byte sequences.

use crate::protocol::command;

/// Prosign: AR (end of message) — merge 'A' + 'R'
pub const PROSIGN_AR: (u8, u8) = (b'A', b'R');
/// Prosign: SK (end of contact) — merge 'S' + 'K'
pub const PROSIGN_SK: (u8, u8) = (b'S', b'K');
/// Prosign: BT (separator/break) — merge 'B' + 'T'
pub const PROSIGN_BT: (u8, u8) = (b'B', b'T');
/// Prosign: KN (go ahead, named station only) — merge 'K' + 'N'
pub const PROSIGN_KN: (u8, u8) = (b'K', b'N');
/// Prosign: AS (wait) — merge 'A' + 'S'
pub const PROSIGN_AS: (u8, u8) = (b'A', b'S');

/// Parse a prosign name to its component letters.
fn parse_prosign(name: &str) -> Option<(u8, u8)> {
    match name.to_uppercase().as_str() {
        "AR" => Some(PROSIGN_AR),
        "SK" => Some(PROSIGN_SK),
        "BT" => Some(PROSIGN_BT),
        "KN" => Some(PROSIGN_KN),
        "AS" => Some(PROSIGN_AS),
        _ => None,
    }
}

/// Build a contest CW message from a template string.
///
/// Supports:
/// - Plain text: sent as-is
/// - `<AR>`, `<SK>`, `<BT>`, `<KN>`, `<AS>`: prosign merge commands
/// - `{20}`: buffered speed change to 20 WPM
/// - `{0}` or `{}`: cancel buffered speed change (restore original)
///
/// Returns the byte sequence ready for serial transmission.
///
/// # Examples
///
/// ```
/// use winkey::message::build_contest_message;
/// let bytes = build_contest_message("CQ TEST K1EL <AR>");
/// assert!(!bytes.is_empty());
/// ```
pub fn build_contest_message(template: &str) -> Vec<u8> {
    let mut output = Vec::new();
    let mut chars = template.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            '<' => {
                // Parse prosign: <AR>, <SK>, etc.
                chars.next(); // consume '<'
                let name: String = chars.by_ref().take_while(|&c| c != '>').collect();
                if let Some((c1, c2)) = parse_prosign(&name) {
                    output.extend_from_slice(&command::buffered_merge(c1, c2));
                }
                // If unknown prosign, silently skip
            }
            '{' => {
                // Parse speed change: {20}, {0}, {}
                chars.next(); // consume '{'
                let num_str: String = chars.by_ref().take_while(|&c| c != '}').collect();
                let wpm: u8 = num_str.trim().parse().unwrap_or(0);
                if wpm == 0 {
                    output.extend_from_slice(&command::cancel_buffered_speed());
                } else {
                    output.extend_from_slice(&command::buffered_speed_change(wpm));
                }
            }
            _ => {
                chars.next();
                // Accumulate plain text characters
                let upper = ch.to_ascii_uppercase() as u8;
                output.push(upper);
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prosign_constants() {
        assert_eq!(PROSIGN_AR, (b'A', b'R'));
        assert_eq!(PROSIGN_SK, (b'S', b'K'));
        assert_eq!(PROSIGN_BT, (b'B', b'T'));
        assert_eq!(PROSIGN_KN, (b'K', b'N'));
        assert_eq!(PROSIGN_AS, (b'A', b'S'));
    }

    #[test]
    fn simple_text() {
        let bytes = build_contest_message("CQ TEST");
        assert_eq!(bytes, b"CQ TEST");
    }

    #[test]
    fn lowercase_converted() {
        let bytes = build_contest_message("cq test");
        assert_eq!(bytes, b"CQ TEST");
    }

    #[test]
    fn with_prosign() {
        let bytes = build_contest_message("CQ TEST <AR>");
        // "CQ TEST " + merge(A, R) — 0x1B = Merge Letters
        assert_eq!(&bytes[..8], b"CQ TEST ");
        assert_eq!(&bytes[8..], &[0x1B, b'A', b'R']);
    }

    #[test]
    fn with_speed_change() {
        let bytes = build_contest_message("5NN{20}TU");
        // "5NN" + speed_change(20) + "TU"
        assert_eq!(&bytes[0..3], b"5NN");
        assert_eq!(&bytes[3..5], &[0x1C, 20]);
        assert_eq!(&bytes[5..7], b"TU");
    }

    #[test]
    fn cancel_speed_change() {
        let bytes = build_contest_message("5NN{0}");
        assert_eq!(&bytes[0..3], b"5NN");
        assert_eq!(&bytes[3..4], &[0x1E]);
    }

    #[test]
    fn cancel_speed_empty_braces() {
        let bytes = build_contest_message("5NN{}");
        assert_eq!(&bytes[0..3], b"5NN");
        assert_eq!(&bytes[3..4], &[0x1E]);
    }

    #[test]
    fn multiple_prosigns() {
        let bytes = build_contest_message("<BT>K1EL<SK>");
        assert_eq!(&bytes[0..3], &[0x1B, b'B', b'T']);
        assert_eq!(&bytes[3..7], b"K1EL");
        assert_eq!(&bytes[7..10], &[0x1B, b'S', b'K']);
    }

    #[test]
    fn mixed_speed_and_prosigns() {
        let bytes = build_contest_message("{28}CQ TEST K1EL{20} 5NN<AR>");
        // {28} = [0x1C, 28]
        assert_eq!(bytes[0], 0x1C);
        assert_eq!(bytes[1], 28);
        // CQ TEST K1EL
        assert_eq!(&bytes[2..14], b"CQ TEST K1EL");
        // {20} = [0x1C, 20]
        assert_eq!(bytes[14], 0x1C);
        assert_eq!(bytes[15], 20);
        // " 5NN"
        assert_eq!(&bytes[16..20], b" 5NN");
        // <AR> = [0x1B, 'A', 'R'] — 0x1B = Merge Letters
        assert_eq!(&bytes[20..23], &[0x1B, b'A', b'R']);
    }

    #[test]
    fn unknown_prosign_skipped() {
        let bytes = build_contest_message("CQ<XX>TEST");
        // <XX> is unknown, silently skipped
        assert_eq!(bytes, b"CQTEST");
    }

    #[test]
    fn empty_message() {
        let bytes = build_contest_message("");
        assert!(bytes.is_empty());
    }
}
