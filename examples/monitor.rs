//! Monitor keyer events (status changes, echo, speed pot, paddle break-in).
//!
//! Usage: cargo run --example monitor -- /dev/ttyUSB0

use winkey::{Keyer, KeyerEvent, WinKeyerBuilder};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <port>", args[0]);
        std::process::exit(1);
    }

    let port = &args[1];

    let keyer = WinKeyerBuilder::new(port)
        .speed(20)
        .build()
        .await?;

    println!("Connected: {}", keyer.info().name);
    println!("Monitoring events (Ctrl-C to quit)...\n");

    let mut rx = keyer.subscribe();

    loop {
        match rx.recv().await {
            Ok(event) => match event {
                KeyerEvent::StatusChanged(status) => {
                    println!(
                        "Status: busy={} keydown={} xoff={} breakin={} waiting={}",
                        status.busy, status.keydown, status.xoff,
                        status.breakin, status.waiting
                    );
                }
                KeyerEvent::SpeedPotChanged { wpm } => {
                    println!("Speed pot: {wpm} WPM");
                }
                KeyerEvent::CharacterSent(ch) => {
                    print!("{ch}");
                }
                KeyerEvent::PaddleBreakIn => {
                    println!("\n[PADDLE BREAK-IN]");
                }
                KeyerEvent::Connected => {
                    println!("[CONNECTED]");
                }
                KeyerEvent::Disconnected => {
                    println!("[DISCONNECTED]");
                    break;
                }
            },
            Err(e) => {
                eprintln!("Event error: {e}");
                break;
            }
        }
    }

    keyer.close().await?;
    Ok(())
}
