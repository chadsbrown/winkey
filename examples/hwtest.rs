//! Automated WinKeyer hardware test suite.
//!
//! Runs through a checklist of features and reports pass/fail for each.
//!
//! Usage: cargo run --example hwtest -- /dev/ttyUSB0 [--speed 20] [--no-sidetone]

use std::time::Duration;

use winkey::{Keyer, KeyerEvent, PinConfig, WinKeyerBuilder};

struct TestRunner {
    passed: u32,
    failed: u32,
    skipped: u32,
}

impl TestRunner {
    fn new() -> Self {
        Self {
            passed: 0,
            failed: 0,
            skipped: 0,
        }
    }

    fn pass(&mut self, name: &str) {
        self.passed += 1;
        println!("  PASS  {name}");
    }

    fn fail(&mut self, name: &str, reason: &str) {
        self.failed += 1;
        println!("  FAIL  {name}: {reason}");
    }

    fn skip(&mut self, name: &str, reason: &str) {
        self.skipped += 1;
        println!("  SKIP  {name}: {reason}");
    }

    fn summary(&self) {
        println!();
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!(
            "Results: {} passed, {} failed, {} skipped",
            self.passed, self.failed, self.skipped
        );
        if self.failed == 0 {
            println!("All tests passed!");
        } else {
            println!("{} test(s) FAILED", self.failed);
        }
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("winkey=debug")
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <port> [--speed <wpm>] [--no-sidetone]", args[0]);
        eprintln!("Example: {} /dev/ttyUSB0 --speed 25", args[0]);
        std::process::exit(1);
    }

    let port = &args[1];
    let speed: u8 = args
        .iter()
        .position(|a| a == "--speed")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let no_sidetone = args.iter().any(|a| a == "--no-sidetone");

    let mut t = TestRunner::new();

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("WinKeyer Hardware Test Suite");
    println!(
        "Port: {port}  Speed: {speed} WPM  Sidetone: {}",
        if no_sidetone { "OFF" } else { "ON" }
    );
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();

    // ── Test 1: Connect ─────────────────────────────────────────
    println!("[1/10] Connect and version detection");
    let mut builder = WinKeyerBuilder::new(port).speed(speed);
    if no_sidetone {
        builder = builder.pin_config(PinConfig::PTT_ENABLE);
    }
    let keyer = match builder.build().await {
        Ok(k) => {
            let ver = k.version();
            t.pass(&format!("connect (version: {ver:?})"));
            println!("        info: {}", k.info().name);
            k
        }
        Err(e) => {
            t.fail("connect", &e.to_string());
            t.summary();
            std::process::exit(1);
        }
    };

    // ── Test 2: Echo test ───────────────────────────────────────
    println!("[2/10] Echo test");
    match keyer.echo_test(0x55).await {
        Ok(v) if v == 0x55 => t.pass("echo test (0x55)"),
        Ok(v) => t.fail("echo test", &format!("expected 0x55, got 0x{v:02X}")),
        Err(e) => t.fail("echo test", &e.to_string()),
    }

    // ── Test 3: Speed set/get ───────────────────────────────────
    println!("[3/10] Speed set/get");
    match keyer.set_speed(25).await {
        Ok(()) => match keyer.get_speed().await {
            Ok(25) => t.pass("speed set/get (25 WPM)"),
            Ok(v) => t.fail("speed get", &format!("expected 25, got {v}")),
            Err(e) => t.fail("speed get", &e.to_string()),
        },
        Err(e) => t.fail("speed set", &e.to_string()),
    }
    // Restore original speed
    let _ = keyer.set_speed(speed).await;

    // ── Test 4: Invalid speed rejected ──────────────────────────
    println!("[4/10] Invalid speed rejection");
    match keyer.set_speed(3).await {
        Err(_) => t.pass("invalid speed rejected (3 WPM)"),
        Ok(()) => t.fail("invalid speed", "accepted speed 3, should reject"),
    }

    // ── Test 5: Send CW + echo events ──────────────────────────
    println!("[5/10] Send CW and verify echo events");
    {
        let mut rx = keyer.subscribe();
        if let Err(e) = keyer.send_message("E").await {
            t.fail("send CW", &e.to_string());
        } else {
            // Wait for echo
            let mut got_echo = false;
            let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
            while tokio::time::Instant::now() < deadline {
                match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
                    Ok(Ok(KeyerEvent::CharacterSent(ch))) => {
                        println!("        echo: '{ch}'");
                        got_echo = true;
                        break;
                    }
                    Ok(Ok(_)) => continue,
                    _ => break,
                }
            }
            if got_echo {
                t.pass("send CW + echo");
            } else {
                t.fail("send CW", "no echo event received within 3s");
            }
        }
        // Wait for the character to finish
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // ── Test 6: Tune on/off ─────────────────────────────────────
    println!("[6/10] Tune mode (key down 500ms)");
    match keyer.set_tune(true).await {
        Ok(()) => {
            tokio::time::sleep(Duration::from_millis(500)).await;
            match keyer.set_tune(false).await {
                Ok(()) => t.pass("tune on/off"),
                Err(e) => t.fail("tune off", &e.to_string()),
            }
        }
        Err(e) => t.fail("tune on", &e.to_string()),
    }
    tokio::time::sleep(Duration::from_millis(200)).await;

    // ── Test 7: Abort ───────────────────────────────────────────
    println!("[7/10] Send + abort");
    if let Err(e) = keyer.send_message("TESTING ABORT").await {
        t.fail("abort (send)", &e.to_string());
    } else {
        tokio::time::sleep(Duration::from_millis(100)).await;
        match keyer.abort().await {
            Ok(()) => t.pass("abort"),
            Err(e) => t.fail("abort", &e.to_string()),
        }
    }
    tokio::time::sleep(Duration::from_millis(200)).await;

    // ── Test 8: Prosign ─────────────────────────────────────────
    println!("[8/10] Prosign (AR)");
    match keyer.send_prosign(b'A', b'R').await {
        Ok(()) => {
            tokio::time::sleep(Duration::from_secs(1)).await;
            t.pass("prosign AR");
        }
        Err(e) => t.fail("prosign", &e.to_string()),
    }

    // ── Test 9: Buffered speed change ───────────────────────────
    println!("[9/10] Buffered speed change");
    match keyer.set_buffered_speed(15).await {
        Ok(()) => {
            if let Err(e) = keyer.send_message("E").await {
                t.fail("buffered speed (send)", &e.to_string());
            } else {
                tokio::time::sleep(Duration::from_secs(1)).await;
                match keyer.cancel_buffered_speed().await {
                    Ok(()) => t.pass("buffered speed change + cancel"),
                    Err(e) => t.fail("buffered speed cancel", &e.to_string()),
                }
            }
        }
        Err(e) => t.fail("buffered speed", &e.to_string()),
    }

    // ── Test 10: Speed pot read ─────────────────────────────────
    println!("[10/10] Speed pot event (2s window)");
    {
        let mut rx = keyer.subscribe();
        let mut got_pot = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {
                Ok(Ok(KeyerEvent::SpeedPotChanged { wpm })) => {
                    println!("        pot: {wpm} WPM");
                    got_pot = true;
                    break;
                }
                Ok(Ok(_)) => continue,
                _ => continue,
            }
        }
        if got_pot {
            t.pass("speed pot event");
        } else {
            t.skip(
                "speed pot event",
                "no pot change in 2s (normal if pot is idle or not present)",
            );
        }
    }

    // ── Cleanup ─────────────────────────────────────────────────
    println!();
    println!("Closing connection...");
    if let Err(e) = keyer.close().await {
        eprintln!("Warning: close error: {e}");
    }

    t.summary();
    std::process::exit(if t.failed > 0 { 1 } else { 0 });
}
