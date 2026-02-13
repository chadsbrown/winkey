# winkey

Async Rust driver for the [K1EL WinKeyer](https://www.hamcrafters2.com/) (WK2, WK3, WK3.1) CW keyer. Provides a high-level API for sending Morse code, managing keyer settings, and receiving events over a serial connection.

Built on Tokio. Protocol implementation follows the K1EL WK3 Datasheet v1.3.

## Quick start

```rust
use winkey::{Keyer, WinKeyerBuilder};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
        .speed(25)
        .build()
        .await?;

    println!("Connected: {}", keyer.info().name);

    keyer.send_message("CQ TEST DE K1EL").await?;

    // Wait for the message to finish
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    keyer.close().await?;
    Ok(())
}
```

## Builder options

```rust
let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
    .speed(28)                           // WPM (5-99)
    .paddle_mode(PaddleMode::IambicB)    // IambicA, IambicB, Ultimatic, Bug
    .contest_spacing(true)               // Shortened wordspace
    .auto_space(true)                    // Auto letter spacing
    .sidetone(800)                       // Frequency in Hz (500-4000)
    .weight(50)                          // 10-90, default 50
    .farnsworth(15)                      // Farnsworth speed (0 = off)
    .ptt_lead_in_ms(50)                  // PTT lead-in (ms)
    .ptt_tail_ms(30)                     // PTT tail (ms)
    .build()
    .await?;
```

## Keyer trait

The `Keyer` trait provides a backend-agnostic interface. Code written against `dyn Keyer` can work with any CW keyer backend.

```rust
use winkey::Keyer;

async fn run_cw(keyer: &dyn Keyer) -> winkey::Result<()> {
    keyer.set_speed(30).await?;
    keyer.send_message("TEST").await?;
    keyer.set_tune(true).await?;    // Key down
    keyer.set_tune(false).await?;   // Key up
    keyer.abort().await?;           // Cancel and clear buffer
    Ok(())
}
```

## Events

Subscribe to real-time events from the keyer:

```rust
let mut rx = keyer.subscribe();

tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        match event {
            KeyerEvent::CharacterSent(ch) => print!("{ch}"),
            KeyerEvent::StatusChanged(s)  => println!("busy={}", s.busy),
            KeyerEvent::SpeedPotChanged { wpm } => println!("{wpm} WPM"),
            KeyerEvent::PaddleBreakIn     => println!("[BREAK-IN]"),
            _ => {}
        }
    }
});
```

## WinKeyer-specific features

Beyond the `Keyer` trait, `WinKeyer` exposes hardware-specific methods:

```rust
keyer.send_prosign(b'A', b'R').await?;       // AR prosign
keyer.set_buffered_speed(15).await?;          // Speed change in buffer
keyer.cancel_buffered_speed().await?;         // Restore original speed
keyer.set_sidetone(1000).await?;              // Sidetone frequency (Hz)
keyer.set_weight(55).await?;                  // Keying weight
keyer.set_ratio(45).await?;                   // Dit/dah ratio
keyer.set_farnsworth(12).await?;              // Farnsworth speed
keyer.echo_test(0x55).await?;                 // Echo test
```

## Contest messages

Build CW messages with inline prosigns and speed changes:

```rust
use winkey::message::build_contest_message;

let bytes = build_contest_message("{28}CQ TEST K1EL{20} 5NN TU{0} <AR>");
keyer.raw_write(&bytes).await?;
```

- `<AR>`, `<SK>`, `<BT>`, `<KN>`, `<AS>` — prosigns
- `{28}` — buffered speed change to 28 WPM
- `{0}` or `{}` — cancel buffered speed change

## Examples

```sh
# Send a CW message
cargo run --example send_cw -- /dev/ttyUSB0 "CQ TEST"

# Interactive terminal (type text, use /commands)
cargo run --example interactive -- /dev/ttyUSB0 --speed 25

# TUI settings app (adjust speed, weight, sidetone, etc.)
cargo run --example tui -- /dev/ttyUSB0

# Monitor keyer events
cargo run --example monitor -- /dev/ttyUSB0

# Hardware test suite
cargo run --example hwtest -- /dev/ttyUSB0
```

## Hardware

Tested with WKUSB (WK3.1). Should work with any K1EL WinKeyer that supports host mode at 1200 baud, including WK2 and WK3.

## License

MIT
