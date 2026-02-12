//! Simple example: connect to WinKeyer and send a CW message.
//!
//! Usage: cargo run --example send_cw -- /dev/ttyUSB0 "CQ TEST K1EL"

use winkey::{Keyer, WinKeyerBuilder};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <port> <message>", args[0]);
        eprintln!("Example: {} /dev/ttyUSB0 \"CQ TEST K1EL\"", args[0]);
        std::process::exit(1);
    }

    let port = &args[1];
    let message = &args[2];

    let keyer = WinKeyerBuilder::new(port)
        .speed(25)
        .build()
        .await?;

    println!("Connected: {}", keyer.info().name);
    println!("Sending: {message}");

    keyer.send_message(message).await?;

    // Wait a bit for the message to finish sending
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    keyer.close().await?;
    println!("Done.");
    Ok(())
}
