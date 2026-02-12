pub mod builder;
pub mod error;
pub mod event;
pub(crate) mod io;
pub mod keyer;
pub mod message;
pub mod protocol;
pub mod transport;
pub mod winkeyer;

pub use builder::WinKeyerBuilder;
pub use error::{Error, Result};
pub use event::{KeyerEvent, KeyerStatus};
pub use keyer::{Keyer, KeyerCapabilities, KeyerInfo};
pub use protocol::types::{
    LoadDefaults, ModeRegister, PaddleMode, PinConfig, WinKeyerVersion,
};
pub use transport::MockPort;
pub use winkeyer::WinKeyer;
