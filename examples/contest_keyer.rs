//! Contest keyer example: demonstrates prosigns, speed changes, and the
//! contest message builder.
//!
//! Usage: cargo run --example contest_keyer -- /dev/ttyUSB0

use winkey::{Keyer, KeyerEvent, PaddleMode, WinKeyerBuilder};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <port>", args[0]);
        std::process::exit(1);
    }

    let port = &args[1];

    // Build with contest-optimized settings
    let keyer = WinKeyerBuilder::new(port)
        .speed(28)
        .paddle_mode(PaddleMode::IambicB)
        .contest_spacing(true)
        .ptt_lead_in_ms(40)
        .ptt_tail_ms(30)
        .build()
        .await?;

    println!("Connected: {}", keyer.info().name);

    // Monitor events in background
    let mut rx = keyer.subscribe();
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            match event {
                KeyerEvent::CharacterSent(ch) => print!("{ch}"),
                KeyerEvent::PaddleBreakIn => println!("\n[BREAK-IN - message aborted]"),
                _ => {}
            }
        }
    });

    // Send CQ using the contest message builder
    let cq_msg = winkey::message::build_contest_message(
        "CQ TEST K1EL K1EL TEST <AR>"
    );
    keyer.raw_write(&cq_msg).await?;

    // Wait for message to complete
    tokio::time::sleep(std::time::Duration::from_secs(8)).await;

    // Send exchange with speed change
    let exchange = winkey::message::build_contest_message(
        "5NN{20}TU{0}"
    );
    keyer.raw_write(&exchange).await?;

    tokio::time::sleep(std::time::Duration::from_secs(4)).await;

    // Demonstrate prosign
    keyer.send_prosign(b'A', b'R').await?;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    keyer.close().await?;
    println!("\nDone.");
    Ok(())
}
