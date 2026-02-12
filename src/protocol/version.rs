//! Version detection and capability gating.

use crate::protocol::types::WinKeyerVersion;

/// Capabilities available at a given WinKeyer version level.
#[derive(Debug, Clone)]
pub struct VersionCapabilities {
    pub wk3_mode: bool,
    pub read_vcc: bool,
    pub extended_serial: bool,
}

impl VersionCapabilities {
    /// Determine capabilities from a detected version.
    pub fn from_version(version: WinKeyerVersion) -> Self {
        match version {
            WinKeyerVersion::Wk2 => Self {
                wk3_mode: false,
                read_vcc: false,
                extended_serial: false,
            },
            WinKeyerVersion::Wk3 => Self {
                wk3_mode: true,
                read_vcc: true,
                extended_serial: false,
            },
            WinKeyerVersion::Wk31 => Self {
                wk3_mode: true,
                read_vcc: true,
                extended_serial: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wk2_capabilities() {
        let caps = VersionCapabilities::from_version(WinKeyerVersion::Wk2);
        assert!(!caps.wk3_mode);
        assert!(!caps.read_vcc);
        assert!(!caps.extended_serial);
    }

    #[test]
    fn wk3_capabilities() {
        let caps = VersionCapabilities::from_version(WinKeyerVersion::Wk3);
        assert!(caps.wk3_mode);
        assert!(caps.read_vcc);
        assert!(!caps.extended_serial);
    }

    #[test]
    fn wk31_capabilities() {
        let caps = VersionCapabilities::from_version(WinKeyerVersion::Wk31);
        assert!(caps.wk3_mode);
        assert!(caps.read_vcc);
        assert!(caps.extended_serial);
    }
}
