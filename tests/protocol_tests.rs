//! Protocol-level tests: command encoding, response decoding, version detection.

use winkey::protocol::command;
use winkey::protocol::response::{self, ResponseByte};
use winkey::protocol::types::*;
use winkey::{KeyerStatus, LoadDefaults};

#[test]
fn full_handshake_byte_sequence() {
    // Defensive close
    let close = command::admin_host_close();
    assert_eq!(close, [0x00, 0x03]);

    // Host open
    let open = command::admin_host_open();
    assert_eq!(open, [0x00, 0x02]);

    // WK2 mode
    let wk2 = command::admin_set_wk2_mode();
    assert_eq!(wk2, [0x00, 0x0B]);

    // WK3 mode
    let wk3 = command::admin_set_wk3_mode();
    assert_eq!(wk3, [0x00, 0x14]);

    // Load defaults with custom params
    let mut defaults = LoadDefaults::default();
    defaults.speed_wpm = 28;
    defaults.lead_in_time = 4;
    defaults.tail_time = 3;
    let cmd = command::load_defaults(&defaults);
    assert_eq!(cmd[0], 0x0F);
    assert_eq!(cmd[2], 28); // speed
    assert_eq!(cmd[5], 4);  // lead-in
    assert_eq!(cmd[6], 3);  // tail
}

#[test]
fn version_detection_all_known() {
    // WK2 versions
    for v in 20..=23 {
        let ver = WinKeyerVersion::from_version_byte(v);
        assert_eq!(ver, Some(WinKeyerVersion::Wk2));
        assert!(!ver.unwrap().supports_wk3());
    }

    // WK3
    let ver = WinKeyerVersion::from_version_byte(30);
    assert_eq!(ver, Some(WinKeyerVersion::Wk3));
    assert!(ver.unwrap().supports_wk3());

    // WK3.1
    let ver = WinKeyerVersion::from_version_byte(31);
    assert_eq!(ver, Some(WinKeyerVersion::Wk31));
    assert!(ver.unwrap().supports_wk3());

    // Unknown
    assert!(WinKeyerVersion::from_version_byte(0).is_none());
    assert!(WinKeyerVersion::from_version_byte(19).is_none());
    assert!(WinKeyerVersion::from_version_byte(24).is_none());
    assert!(WinKeyerVersion::from_version_byte(29).is_none());
    assert!(WinKeyerVersion::from_version_byte(32).is_none());
}

#[test]
fn response_byte_classification_full_range() {
    // Echo range: 0x00-0x7F
    for b in 0x00..=0x7F {
        let r = response::classify_byte(b);
        assert!(matches!(r, ResponseByte::Echo(_)));
    }

    // Speed pot range: 0x80-0xBF
    for b in 0x80..=0xBF {
        let r = response::classify_byte(b);
        assert!(matches!(r, ResponseByte::SpeedPot { .. }));
    }

    // Status range: 0xC0-0xFF
    for b in 0xC0..=0xFF {
        let r = response::classify_byte(b);
        assert!(matches!(r, ResponseByte::Status(_)));
    }
}

#[test]
fn status_byte_bit_extraction() {
    // Test individual bits per WK3 Datasheet v1.3, Tables 14-15
    let cases: &[(u8, &str, fn(&KeyerStatus) -> bool)] = &[
        (0xC1, "xoff", |s| s.xoff),       // bit 0
        (0xC2, "breakin", |s| s.breakin),  // bit 1
        (0xC4, "busy", |s| s.busy),        // bit 2
        (0xC8, "keydown", |s| s.keydown),  // bit 3
        (0xD0, "waiting", |s| s.waiting),  // bit 4
    ];

    for (byte, name, check) in cases {
        let status = KeyerStatus::from_status_byte(*byte);
        assert!(check(&status), "{name} should be set for 0x{byte:02X}");
    }
}

#[test]
fn mode_register_combinations() {
    let mode = ModeRegister::SERIAL_ECHO
        | ModeRegister::PADDLE_ECHO
        | ModeRegister::CONTEST_SPACING;
    let byte = mode.with_paddle_mode(PaddleMode::IambicA);
    // SERIAL_ECHO=0x04, PADDLE_ECHO=0x40, CONTEST_SPACING=0x01, IambicA=0x10
    assert_eq!(byte, 0x04 | 0x40 | 0x01 | 0x10);

    let mode = ModeRegister::SERIAL_ECHO | ModeRegister::AUTO_SPACE;
    let byte = mode.with_paddle_mode(PaddleMode::Bug);
    // SERIAL_ECHO=0x04, AUTO_SPACE=0x02, Bug=0x30
    assert_eq!(byte, 0x04 | 0x02 | 0x30);
}

#[test]
fn pin_config_combinations() {
    // KEY_OUTPUT_2 is bit 2 (0x04) per WK3 Datasheet Table 10
    let config = PinConfig::PTT_ENABLE | PinConfig::SIDETONE_ENABLE | PinConfig::KEY_OUTPUT_2;
    assert_eq!(config.bits(), 0x01 | 0x02 | 0x04);
}

#[test]
fn text_validation_edge_cases() {
    assert!(command::validate_cw_text("").is_ok());
    assert!(command::validate_cw_text("A").is_ok());
    assert!(command::validate_cw_text("ABCDEFGHIJKLMNOPQRSTUVWXYZ").is_ok());
    assert!(command::validate_cw_text("0123456789").is_ok());
    assert!(command::validate_cw_text(".,?/!=+-:;'\"()@&_").is_ok());

    // Invalid characters
    assert!(command::validate_cw_text("\n").is_err());
    assert!(command::validate_cw_text("\t").is_err());
    assert!(command::validate_cw_text("~").is_err());
    assert!(command::validate_cw_text("`").is_err());
    assert!(command::validate_cw_text("#").is_err());
    assert!(command::validate_cw_text("$").is_err());
    assert!(command::validate_cw_text("%").is_err());
    assert!(command::validate_cw_text("^").is_err());
    assert!(command::validate_cw_text("*").is_err());
    assert!(command::validate_cw_text("[").is_err());
    assert!(command::validate_cw_text("]").is_err());
    assert!(command::validate_cw_text("{").is_err());
    assert!(command::validate_cw_text("}").is_err());
    assert!(command::validate_cw_text("|").is_err());
    assert!(command::validate_cw_text("\\").is_err());
    assert!(command::validate_cw_text("<").is_err());
    assert!(command::validate_cw_text(">").is_err());
}

#[test]
fn contest_message_builder_complex() {
    use winkey::message::build_contest_message;

    // Full contest exchange
    let msg = build_contest_message("{28}CQ TEST K1EL K1EL TEST{0} <AR>");
    assert!(msg.len() > 20);
    // Starts with buffered speed change
    assert_eq!(msg[0], 0x1C);
    assert_eq!(msg[1], 28);
    // Ends with prosign AR (0x1B = Merge Letters)
    let tail = &msg[msg.len() - 3..];
    assert_eq!(tail, &[0x1B, b'A', b'R']);
}
