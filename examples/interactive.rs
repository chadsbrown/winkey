//! Interactive WinKeyer terminal.
//!
//! Type text to send CW. Special commands:
//!
//!   /speed <wpm>   Set speed
//!   /tune          Toggle tune mode
//!   /abort         Abort current message
//!   /prosign <XX>  Send prosign (AR, SK, BT, KN, AS)
//!   /echo <hex>    Echo test
//!   /weight <n>    Set weight (10-90)
//!   /sidetone <n>  Set sidetone frequency in Hz (500-4000)
//!   /farnsworth <n> Set Farnsworth speed (0=off)
//!   /pause         Toggle pause
//!   /status        Request status
//!   /quit          Close and exit
//!
//! Usage: cargo run --example interactive -- /dev/ttyUSB0 [--speed 20]

use std::io::Write;
use std::time::Duration;

use tokio::io::AsyncBufReadExt;

use winkey::{Keyer, KeyerEvent, WinKeyerBuilder};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <port> [--speed <wpm>]", args[0]);
        std::process::exit(1);
    }

    let port = &args[1];
    let speed: u8 = args
        .iter()
        .position(|a| a == "--speed")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    println!("Connecting to {port}...");
    let keyer = WinKeyerBuilder::new(port).speed(speed).build().await?;

    println!("Connected: {}", keyer.info().name);
    println!("Speed: {speed} WPM");
    println!();
    println!("Type text to send CW. Commands start with /");
    println!("Type /help for command list, /quit to exit.");
    println!();

    // Spawn event monitor
    let mut event_rx = keyer.subscribe();
    tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(event) => match event {
                    KeyerEvent::StatusChanged(s) => {
                        if s.busy || s.keydown || s.xoff {
                            eprint!(
                                "\r  [status: busy={} key={} xoff={}]\r\n> ",
                                s.busy, s.keydown, s.xoff
                            );
                            let _ = std::io::stderr().flush();
                        }
                    }
                    KeyerEvent::SpeedPotChanged { wpm } => {
                        eprint!("\r  [pot: {wpm} WPM]\r\n> ");
                        let _ = std::io::stderr().flush();
                    }
                    KeyerEvent::CharacterSent(ch) => {
                        eprint!("{ch}");
                        let _ = std::io::stderr().flush();
                    }
                    KeyerEvent::PaddleBreakIn => {
                        eprint!("\r  [PADDLE BREAK-IN]\r\n> ");
                        let _ = std::io::stderr().flush();
                    }
                    KeyerEvent::Disconnected => {
                        eprintln!("\r  [DISCONNECTED]");
                        break;
                    }
                    KeyerEvent::Connected => {}
                },
                Err(_) => break,
            }
        }
    });

    let stdin = tokio::io::BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();

    let mut tune_on = false;
    let mut paused = false;

    loop {
        eprint!("> ");
        let _ = std::io::stderr().flush();

        let line = match lines.next_line().await? {
            Some(l) => l,
            None => break,
        };

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('/') {
            let parts: Vec<&str> = line.splitn(2, ' ').collect();
            let cmd = parts[0];
            let arg = parts.get(1).copied().unwrap_or("");

            match cmd {
                "/help" => {
                    println!("Commands:");
                    println!("  /speed <wpm>     Set speed (5-99)");
                    println!("  /tune            Toggle tune mode");
                    println!("  /abort           Abort current message");
                    println!("  /prosign <XX>    Send prosign (AR, SK, BT, KN, AS)");
                    println!("  /echo <hex>      Echo test (e.g. /echo 55)");
                    println!("  /weight <n>      Set weight (10-90)");
                    println!("  /sidetone <n>    Set sidetone Hz (500-4000)");
                    println!("  /farnsworth <n>  Set Farnsworth speed (0=off)");
                    println!("  /pause           Toggle pause");
                    println!("  /msg <template>  Send contest message (supports <AR>, {{20}})");
                    println!("  /quit            Close and exit");
                }
                "/speed" => {
                    if let Ok(wpm) = arg.parse::<u8>() {
                        match keyer.set_speed(wpm).await {
                            Ok(()) => println!("Speed set to {wpm} WPM"),
                            Err(e) => eprintln!("Error: {e}"),
                        }
                    } else {
                        eprintln!("Usage: /speed <wpm>");
                    }
                }
                "/tune" => {
                    tune_on = !tune_on;
                    match keyer.set_tune(tune_on).await {
                        Ok(()) => println!("Tune: {}", if tune_on { "ON" } else { "OFF" }),
                        Err(e) => {
                            tune_on = !tune_on;
                            eprintln!("Error: {e}");
                        }
                    }
                }
                "/abort" => match keyer.abort().await {
                    Ok(()) => println!("Aborted"),
                    Err(e) => eprintln!("Error: {e}"),
                },
                "/prosign" => {
                    let arg_upper = arg.to_uppercase();
                    let (c1, c2) = match arg_upper.as_str() {
                        "AR" => (b'A', b'R'),
                        "SK" => (b'S', b'K'),
                        "BT" => (b'B', b'T'),
                        "KN" => (b'K', b'N'),
                        "AS" => (b'A', b'S'),
                        _ => {
                            if arg.len() == 2 {
                                let bytes = arg_upper.as_bytes();
                                (bytes[0], bytes[1])
                            } else {
                                eprintln!("Usage: /prosign <XX> (e.g. AR, SK, BT)");
                                continue;
                            }
                        }
                    };
                    match keyer.send_prosign(c1, c2).await {
                        Ok(()) => println!("Sent prosign {arg_upper}"),
                        Err(e) => eprintln!("Error: {e}"),
                    }
                }
                "/echo" => {
                    let byte = u8::from_str_radix(arg.trim_start_matches("0x"), 16)
                        .unwrap_or(0x55);
                    match keyer.echo_test(byte).await {
                        Ok(v) => {
                            if v == byte {
                                println!("Echo OK: 0x{v:02X}");
                            } else {
                                println!("Echo MISMATCH: sent 0x{byte:02X}, got 0x{v:02X}");
                            }
                        }
                        Err(e) => eprintln!("Error: {e}"),
                    }
                }
                "/weight" => {
                    if let Ok(w) = arg.parse::<u8>() {
                        match keyer.set_weight(w).await {
                            Ok(()) => println!("Weight set to {w}"),
                            Err(e) => eprintln!("Error: {e}"),
                        }
                    } else {
                        eprintln!("Usage: /weight <10-90>");
                    }
                }
                "/sidetone" => {
                    if let Ok(v) = arg.parse::<u16>() {
                        match keyer.set_sidetone(v).await {
                            Ok(()) => println!("Sidetone set to {v} Hz"),
                            Err(e) => eprintln!("Error: {e}"),
                        }
                    } else {
                        eprintln!("Usage: /sidetone <500-4000>");
                    }
                }
                "/farnsworth" => {
                    if let Ok(wpm) = arg.parse::<u8>() {
                        match keyer.set_farnsworth(wpm).await {
                            Ok(()) => {
                                if wpm == 0 {
                                    println!("Farnsworth disabled");
                                } else {
                                    println!("Farnsworth set to {wpm} WPM");
                                }
                            }
                            Err(e) => eprintln!("Error: {e}"),
                        }
                    } else {
                        eprintln!("Usage: /farnsworth <wpm> (0=off)");
                    }
                }
                "/pause" => {
                    paused = !paused;
                    match keyer.set_pause(paused).await {
                        Ok(()) => {
                            println!("Pause: {}", if paused { "ON" } else { "OFF" });
                        }
                        Err(e) => {
                            paused = !paused;
                            eprintln!("Error: {e}");
                        }
                    }
                }
                "/msg" => {
                    if arg.is_empty() {
                        eprintln!("Usage: /msg <template>");
                        eprintln!("  e.g. /msg CQ TEST K1EL <AR>");
                        eprintln!("  e.g. /msg {{28}}5NN TU{{0}}");
                    } else {
                        let bytes = winkey::message::build_contest_message(arg);
                        match keyer.raw_write(&bytes).await {
                            Ok(()) => println!("Sent {} bytes", bytes.len()),
                            Err(e) => eprintln!("Error: {e}"),
                        }
                    }
                }
                "/quit" | "/exit" | "/q" => {
                    break;
                }
                _ => {
                    eprintln!("Unknown command: {cmd} (type /help for list)");
                }
            }
        } else {
            // Plain text: send as CW
            match keyer.send_message(line).await {
                Ok(()) => {}
                Err(e) => eprintln!("Error: {e}"),
            }
        }
    }

    println!("Closing...");
    // Make sure tune is off
    if tune_on {
        let _ = keyer.set_tune(false).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    keyer.close().await?;
    println!("Done.");
    Ok(())
}
