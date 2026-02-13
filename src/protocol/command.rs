//! Command encoding: host → WinKeyer.
//!
//! All functions are pure (no I/O). They return byte arrays or vectors
//! ready for serial transmission.

use crate::protocol::types::LoadDefaults;

// ---------------------------------------------------------------------------
// Admin commands (0x00 prefix)
// ---------------------------------------------------------------------------

/// Admin: Calibrate (0x00 0x00). Sends calibration value.
pub fn admin_calibrate(value: u8) -> [u8; 3] {
    [0x00, 0x00, value]
}

/// Admin: Reset (0x00 0x01). Soft reset WinKeyer.
pub fn admin_reset() -> [u8; 2] {
    [0x00, 0x01]
}

/// Admin: Host Open (0x00 0x02). Opens host mode, returns version byte.
pub fn admin_host_open() -> [u8; 2] {
    [0x00, 0x02]
}

/// Admin: Host Close (0x00 0x03). Closes host mode.
pub fn admin_host_close() -> [u8; 2] {
    [0x00, 0x03]
}

/// Admin: Echo Test (0x00 0x04 value). Echoes value back.
pub fn admin_echo_test(value: u8) -> [u8; 3] {
    [0x00, 0x04, value]
}

/// Admin: Paddle A2D (0x00 0x05). Read paddle A/D value.
pub fn admin_paddle_a2d() -> [u8; 2] {
    [0x00, 0x05]
}

/// Admin: Speed A2D (0x00 0x06). Read speed pot A/D value.
pub fn admin_speed_a2d() -> [u8; 2] {
    [0x00, 0x06]
}

/// Admin: Get Values (0x00 0x07). Read current operating parameters.
pub fn admin_get_values() -> [u8; 2] {
    [0x00, 0x07]
}

/// Admin: Reserved (0x00 0x08).
pub fn admin_reserved() -> [u8; 2] {
    [0x00, 0x08]
}

/// Admin: Get FW Major Rev (0x00 0x09). Read firmware major version.
pub fn admin_get_fw_major_rev() -> [u8; 2] {
    [0x00, 0x09]
}

/// Admin: Set WK1 Mode (0x00 0x0A). Switch to WK1 compatibility mode.
pub fn admin_set_wk1_mode() -> [u8; 2] {
    [0x00, 0x0A]
}

/// Admin: Set WK2 Mode (0x00 0x0B). Switch to WK2 mode.
pub fn admin_set_wk2_mode() -> [u8; 2] {
    [0x00, 0x0B]
}

/// Admin: Dump EEPROM (0x00 0x0C). Dump 256 bytes of EEPROM.
pub fn admin_dump_eeprom() -> [u8; 2] {
    [0x00, 0x0C]
}

/// Admin: Load EEPROM (0x00 0x0D). Load 256 bytes into EEPROM.
pub fn admin_load_eeprom() -> [u8; 2] {
    [0x00, 0x0D]
}

/// Admin: Send MSG (0x00 0x0E slot). Play stored message 1-6.
pub fn admin_send_msg(slot: u8) -> [u8; 3] {
    [0x00, 0x0E, slot]
}

/// Admin: Load X1MODE (0x00 0x0F value). Set X1 mode register.
pub fn admin_load_x1mode(value: u8) -> [u8; 3] {
    [0x00, 0x0F, value]
}

/// Admin: Firmware Update (0x00 0x10). Enter firmware update mode.
pub fn admin_firmware_update() -> [u8; 2] {
    [0x00, 0x10]
}

/// Admin: Set Low Baud (0x00 0x11). Switch to 1200 baud.
pub fn admin_set_low_baud() -> [u8; 2] {
    [0x00, 0x11]
}

/// Admin: Set High Baud (0x00 0x12). Switch to 9600 baud.
pub fn admin_set_high_baud() -> [u8; 2] {
    [0x00, 0x12]
}

/// Admin: Set RTTY Mode Registers (0x00 0x13 P1 P2). WK3.1 only.
pub fn admin_set_rtty_registers(p1: u8, p2: u8) -> [u8; 4] {
    [0x00, 0x13, p1, p2]
}

/// Admin: Set WK3 Mode (0x00 0x14). Switch to WK3 extended mode.
pub fn admin_set_wk3_mode() -> [u8; 2] {
    [0x00, 0x14]
}

/// Admin: Read back VCC (0x00 0x15). Read supply voltage (WK3+).
pub fn admin_read_vcc() -> [u8; 2] {
    [0x00, 0x15]
}

/// Admin: Load X2MODE (0x00 0x16 value). Set X2 extension mode register (WK3 only).
///
/// Bit layout:
/// - Bit 7: Paddle status reporting
/// - Bit 6: Fast command response
/// - Bit 5: Cut 9 (substitute N for 9)
/// - Bit 4: Cut 0 (substitute T for 0)
/// - Bit 3: Paddle-only sidetone
/// - Bit 2: SO2R mode
/// - Bit 1: Paddle mute
/// - Bit 0: Spare
pub fn admin_load_x2mode(value: u8) -> [u8; 3] {
    [0x00, 0x16, value]
}

/// Admin: Get FW Minor Rev (0x00 0x17). Read firmware minor version (WK3+).
pub fn admin_get_fw_minor_rev() -> [u8; 2] {
    [0x00, 0x17]
}

/// Admin: Get IC Type (0x00 0x18). Read IC type identifier (WK3+).
pub fn admin_get_ic_type() -> [u8; 2] {
    [0x00, 0x18]
}

/// Admin: Set Sidetone Volume (0x00 0x19 value). WK3 only.
///
/// Values: 1-2 = low volume, 3-4 = normal (high) volume.
pub fn admin_set_sidetone_volume(value: u8) -> [u8; 3] {
    [0x00, 0x19, value]
}

// ---------------------------------------------------------------------------
// Immediate commands (0x01 - 0x15)
// ---------------------------------------------------------------------------

/// Sidetone Control (0x01 value). Value 1-10 maps to frequency.
pub fn sidetone_control(value: u8) -> [u8; 2] {
    [0x01, value]
}

/// Set WPM Speed (0x02 wpm). Range 5-99.
pub fn set_speed(wpm: u8) -> [u8; 2] {
    [0x02, wpm]
}

/// Set Weighting (0x03 weight). Range 10-90, default 50.
pub fn set_weight(weight: u8) -> [u8; 2] {
    [0x03, weight]
}

/// Set PTT Lead-in/Tail (0x04 lead tail).
/// Values in 10ms units. Lead range 0-250, Tail range 0-250.
pub fn set_ptt_timing(lead_in: u8, tail: u8) -> [u8; 3] {
    [0x04, lead_in, tail]
}

/// Set Speed Pot range (0x05 min range 0).
/// min = minimum WPM, range = WPM span of pot.
/// Third parameter is reserved (must be 0) per WK3 Datasheet v1.3.
pub fn set_speed_pot(min: u8, range: u8) -> [u8; 4] {
    [0x05, min, range, 0]
}

/// Pause output (0x06 state). 1 = pause, 0 = resume.
pub fn set_pause(paused: bool) -> [u8; 2] {
    [0x06, if paused { 1 } else { 0 }]
}

/// Get Speed Pot value (0x07). Returns current pot speed.
pub fn get_speed_pot() -> [u8; 1] {
    [0x07]
}

/// Backspace (0x08). Delete last character from buffer.
pub fn backspace() -> [u8; 1] {
    [0x08]
}

/// Set Pin Configuration (0x09 config).
pub fn set_pin_config(config: u8) -> [u8; 2] {
    [0x09, config]
}

/// Clear Buffer (0x0A). Abort current message and clear send buffer.
pub fn clear_buffer() -> [u8; 1] {
    [0x0A]
}

/// Key Immediate (0x0B state). 1 = key down, 0 = key up (tune mode).
pub fn key_immediate(down: bool) -> [u8; 2] {
    [0x0B, if down { 1 } else { 0 }]
}

/// Set HSCW Speed (0x0C speed). High-speed CW mode.
pub fn set_hscw_speed(speed: u8) -> [u8; 2] {
    [0x0C, speed]
}

/// Set Farnsworth Speed (0x0D wpm). 0 = disable.
pub fn set_farnsworth(wpm: u8) -> [u8; 2] {
    [0x0D, wpm]
}

/// Set WinKeyer Mode Register (0x0E mode).
pub fn set_mode_register(mode: u8) -> [u8; 2] {
    [0x0E, mode]
}

/// Load Defaults (0x0F + 15 bytes).
pub fn load_defaults(defaults: &LoadDefaults) -> [u8; 16] {
    let params = defaults.to_bytes();
    let mut cmd = [0u8; 16];
    cmd[0] = 0x0F;
    cmd[1..16].copy_from_slice(&params);
    cmd
}

/// Set 1st Extension (0x10 value). First dit/dah extension.
pub fn set_first_extension(value: u8) -> [u8; 2] {
    [0x10, value]
}

/// Set Key Compensation (0x11 value).
pub fn set_key_compensation(value: u8) -> [u8; 2] {
    [0x11, value]
}

/// Set Paddle Switchpoint (0x12 value). 10-90, default 50.
pub fn set_paddle_switchpoint(value: u8) -> [u8; 2] {
    [0x12, value]
}

/// Null command (0x13). Does nothing, can be used as keep-alive.
pub fn null_command() -> [u8; 1] {
    [0x13]
}

/// Software Paddle (0x14 state).
/// Bit 0 = dit paddle, bit 1 = dah paddle. 0 = release both.
pub fn software_paddle(dit: bool, dah: bool) -> [u8; 2] {
    let mut state = 0u8;
    if dit {
        state |= 0x01;
    }
    if dah {
        state |= 0x02;
    }
    [0x14, state]
}

/// Request WinKeyer Status (0x15). Requests an immediate status byte.
pub fn request_status() -> [u8; 1] {
    [0x15]
}

// ---------------------------------------------------------------------------
// Buffered commands (0x16 - 0x1F)
// ---------------------------------------------------------------------------

/// Pointer Command (0x16 subcmd). Manipulate input buffer pointers.
///
/// Sub-commands per WK3 Datasheet v1.3:
/// - 0x00: Reset input buffer pointers
/// - 0x01: Move pointer in overwrite mode
/// - 0x02: Move pointer in append mode
/// - 0x03: Add multiple nulls (0x16 0x03 count)
pub fn pointer_cmd(subcmd: u8) -> [u8; 2] {
    [0x16, subcmd]
}

/// Pointer Command with data (0x16 subcmd data...).
pub fn pointer_cmd_with_data(subcmd: u8, data: &[u8]) -> Vec<u8> {
    let mut cmd = Vec::with_capacity(2 + data.len());
    cmd.push(0x16);
    cmd.push(subcmd);
    cmd.extend_from_slice(data);
    cmd
}

/// Buffered PTT on/off (0x18 on_off). 1 = assert PTT, 0 = release.
pub fn buffered_ptt(on: bool) -> [u8; 2] {
    [0x18, if on { 1 } else { 0 }]
}

/// Key Buffered (0x19 seconds). Assert key output for nn seconds (0-99).
pub fn key_buffered(seconds: u8) -> [u8; 2] {
    [0x19, seconds]
}

/// Buffered Wait (0x1A seconds). Insert a timed pause in buffer (0-99 seconds).
pub fn buffered_wait(seconds: u8) -> [u8; 2] {
    [0x1A, seconds]
}

/// Buffered Merge Letters (0x1B c1 c2). Merge two letters into prosign.
pub fn buffered_merge(c1: u8, c2: u8) -> [u8; 3] {
    [0x1B, c1, c2]
}

/// Buffered Speed Change (0x1C wpm). Change speed in buffer. 0 = restore.
pub fn buffered_speed_change(wpm: u8) -> [u8; 2] {
    [0x1C, wpm]
}

/// Buffered HSCW Speed (0x1D speed).
pub fn buffered_hscw_speed(speed: u8) -> [u8; 2] {
    [0x1D, speed]
}

/// Cancel Buffered Speed Change (0x1E). Restore speed after buffer change.
pub fn cancel_buffered_speed() -> [u8; 1] {
    [0x1E]
}

/// Buffered NOP (0x1F). No operation, used as buffer placeholder.
pub fn buffered_nop() -> [u8; 1] {
    [0x1F]
}

// ---------------------------------------------------------------------------
// Dit/Dah Ratio
// ---------------------------------------------------------------------------

/// Set Dit/Dah Ratio (0x17 ratio). Range 33-66, default 50 = 3:1.
pub fn set_ratio(ratio: u8) -> [u8; 2] {
    [0x17, ratio]
}

// ---------------------------------------------------------------------------
// Text encoding
// ---------------------------------------------------------------------------

/// Validate that a string contains only characters WinKeyer can send.
/// Valid characters: A-Z, 0-9, space, and punctuation: . , ? / ! = + - : ; ' " ( ) @ &
pub fn validate_cw_text(text: &str) -> std::result::Result<(), String> {
    for (i, ch) in text.chars().enumerate() {
        if !is_valid_cw_char(ch) {
            return Err(format!("invalid CW character '{}' at position {}", ch, i));
        }
    }
    Ok(())
}

/// Check if a character is valid for WinKeyer CW output.
fn is_valid_cw_char(ch: char) -> bool {
    matches!(ch,
        'A'..='Z' | 'a'..='z' | '0'..='9' | ' '
        | '.' | ',' | '?' | '/' | '!' | '='
        | '+' | '-' | ':' | ';' | '\'' | '"'
        | '(' | ')' | '@' | '&' | '_'
    )
}

/// Encode text as bytes for WinKeyer. Converts to uppercase ASCII.
/// Characters are sent directly (WinKeyer handles Morse encoding).
pub fn encode_text(text: &str) -> Vec<u8> {
    text.to_uppercase().bytes().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::types::LoadDefaults;

    #[test]
    fn admin_commands() {
        assert_eq!(admin_host_open(), [0x00, 0x02]);
        assert_eq!(admin_host_close(), [0x00, 0x03]);
        assert_eq!(admin_reset(), [0x00, 0x01]);
        assert_eq!(admin_set_wk2_mode(), [0x00, 0x0B]);
        assert_eq!(admin_set_wk3_mode(), [0x00, 0x14]);
        assert_eq!(admin_set_high_baud(), [0x00, 0x12]);
        assert_eq!(admin_echo_test(0x42), [0x00, 0x04, 0x42]);
        assert_eq!(admin_send_msg(3), [0x00, 0x0E, 3]);
    }

    #[test]
    fn immediate_commands() {
        assert_eq!(set_speed(28), [0x02, 28]);
        assert_eq!(set_weight(50), [0x03, 50]);
        assert_eq!(set_ptt_timing(4, 3), [0x04, 4, 3]);
        assert_eq!(clear_buffer(), [0x0A]);
        assert_eq!(key_immediate(true), [0x0B, 1]);
        assert_eq!(key_immediate(false), [0x0B, 0]);
        assert_eq!(set_farnsworth(15), [0x0D, 15]);
        assert_eq!(set_pause(true), [0x06, 1]);
        assert_eq!(set_pause(false), [0x06, 0]);
        assert_eq!(request_status(), [0x15]);
    }

    #[test]
    fn software_paddle_encoding() {
        assert_eq!(software_paddle(false, false), [0x14, 0x00]);
        assert_eq!(software_paddle(true, false), [0x14, 0x01]);
        assert_eq!(software_paddle(false, true), [0x14, 0x02]);
        assert_eq!(software_paddle(true, true), [0x14, 0x03]);
    }

    #[test]
    fn buffered_commands() {
        assert_eq!(buffered_speed_change(25), [0x1C, 25]);
        assert_eq!(cancel_buffered_speed(), [0x1E]);
        assert_eq!(buffered_merge(b'A', b'R'), [0x1B, b'A', b'R']);
        assert_eq!(buffered_ptt(true), [0x18, 1]);
        assert_eq!(buffered_ptt(false), [0x18, 0]);
        assert_eq!(key_buffered(5), [0x19, 5]);
        assert_eq!(buffered_wait(5), [0x1A, 5]);
        assert_eq!(buffered_nop(), [0x1F]);
    }

    #[test]
    fn pointer_command_encoding() {
        assert_eq!(pointer_cmd(0x00), [0x16, 0x00]);
        let cmd = pointer_cmd_with_data(0x03, &[5]);
        assert_eq!(cmd, vec![0x16, 0x03, 5]);
    }

    #[test]
    fn load_defaults_encoding() {
        let defaults = LoadDefaults::default();
        let cmd = load_defaults(&defaults);
        assert_eq!(cmd[0], 0x0F);
        assert_eq!(cmd.len(), 16);
        assert_eq!(cmd[2], 20); // speed_wpm
    }

    #[test]
    fn text_validation() {
        assert!(validate_cw_text("CQ TEST K1EL").is_ok());
        assert!(validate_cw_text("5NN TU").is_ok());
        assert!(validate_cw_text("?/!").is_ok());
        assert!(validate_cw_text("hello").is_ok()); // lowercase OK
        assert!(validate_cw_text("CQ~TEST").is_err()); // tilde invalid
        assert!(validate_cw_text("CQ\tTEST").is_err()); // tab invalid
    }

    #[test]
    fn text_encoding() {
        assert_eq!(encode_text("cq test"), b"CQ TEST");
        assert_eq!(encode_text("5NN"), b"5NN");
    }

    #[test]
    fn sidetone_command() {
        assert_eq!(sidetone_control(5), [0x01, 5]);
    }

    #[test]
    fn speed_pot_command() {
        assert_eq!(set_speed_pot(10, 25), [0x05, 10, 25, 0]);
    }
}
