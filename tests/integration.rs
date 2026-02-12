//! Integration tests using MockPort for full handshake + IO task scenarios.

use std::time::Duration;

use winkey::{
    Keyer, KeyerEvent, MockPort, PaddleMode, WinKeyerBuilder, WinKeyerVersion,
};

/// Create a MockPort that delivers a version byte after a delay.
fn mock_wk(version_byte: u8) -> MockPort {
    let mock = MockPort::new();
    let clone = mock.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        clone.queue_read(&[version_byte]);
    });
    mock
}

#[tokio::test]
async fn full_open_close_handshake() {
    let mock = mock_wk(23);
    let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
        .speed(25)
        .build_with_port(mock.clone())
        .await
        .unwrap();

    assert_eq!(keyer.version(), WinKeyerVersion::Wk2);

    keyer.close().await.unwrap();

    // Verify host close was sent
    let written = mock.written_data();
    // Last 2 bytes should be host close (0x00, 0x03)
    let len = written.len();
    assert!(len >= 2);
    assert_eq!(&written[len - 2..], &[0x00, 0x03]);
}

#[tokio::test]
async fn send_message_and_receive_echo() {
    let mock = mock_wk(23);
    let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
        .build_with_port(mock.clone())
        .await
        .unwrap();

    let mut rx = keyer.subscribe();

    // Send a message
    keyer.send_message("CQ").await.unwrap();

    // Give IO task time to write
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Simulate WinKeyer echoing the characters
    mock.queue_read(&[b'C', b'Q']);

    // Receive echo events
    let ev1 = tokio::time::timeout(Duration::from_millis(200), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(ev1, KeyerEvent::CharacterSent('C')));

    let ev2 = tokio::time::timeout(Duration::from_millis(200), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(ev2, KeyerEvent::CharacterSent('Q')));

    keyer.close().await.unwrap();
}

#[tokio::test]
async fn abort_preempts_queued_text() {
    let mock = mock_wk(23);
    let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
        .build_with_port(mock.clone())
        .await
        .unwrap();

    // Queue some text
    keyer.send_message("LONG MESSAGE").await.unwrap();

    // Abort should go via RT channel and take priority
    keyer.abort().await.unwrap();

    // Give IO task time to process
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify clear buffer command (0x0A) was written
    let written = mock.written_data();
    assert!(
        written.contains(&0x0A),
        "clear buffer byte should be present in written data"
    );

    keyer.close().await.unwrap();
}

#[tokio::test]
async fn speed_set_and_get() {
    let mock = mock_wk(23);
    let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
        .speed(20)
        .build_with_port(mock.clone())
        .await
        .unwrap();

    assert_eq!(keyer.get_speed().await.unwrap(), 20);

    keyer.set_speed(35).await.unwrap();
    assert_eq!(keyer.get_speed().await.unwrap(), 35);

    // Verify speed command was sent
    let written = mock.written_data();
    assert!(written.windows(2).any(|w| w == [0x02, 35]));

    keyer.close().await.unwrap();
}

#[tokio::test]
async fn tune_mode() {
    let mock = mock_wk(23);
    let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
        .build_with_port(mock.clone())
        .await
        .unwrap();

    keyer.set_tune(true).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    keyer.set_tune(false).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    let written = mock.written_data();
    assert!(written.windows(2).any(|w| w == [0x0B, 1])); // key down
    assert!(written.windows(2).any(|w| w == [0x0B, 0])); // key up

    keyer.close().await.unwrap();
}

#[tokio::test]
async fn paddle_breakin_event() {
    let mock = mock_wk(23);
    let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
        .build_with_port(mock.clone())
        .await
        .unwrap();

    let mut rx = keyer.subscribe();

    // Send status with no breakin, then with breakin
    mock.queue_read(&[0xC0, 0xC4]);

    // First: StatusChanged (clear)
    let ev1 = tokio::time::timeout(Duration::from_millis(200), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(ev1, KeyerEvent::StatusChanged(s) if !s.breakin));

    // Second: PaddleBreakIn (edge detection)
    let ev2 = tokio::time::timeout(Duration::from_millis(200), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(ev2, KeyerEvent::PaddleBreakIn));

    // Third: StatusChanged (with breakin)
    let ev3 = tokio::time::timeout(Duration::from_millis(200), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(ev3, KeyerEvent::StatusChanged(s) if s.breakin));

    keyer.close().await.unwrap();
}

#[tokio::test]
async fn wk3_version_detection() {
    let mock = mock_wk(30);
    let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
        .build_with_port(mock.clone())
        .await
        .unwrap();
    assert_eq!(keyer.version(), WinKeyerVersion::Wk3);
    keyer.close().await.unwrap();
}

#[tokio::test]
async fn wk31_version_detection() {
    let mock = mock_wk(31);
    let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
        .build_with_port(mock.clone())
        .await
        .unwrap();
    assert_eq!(keyer.version(), WinKeyerVersion::Wk31);
    keyer.close().await.unwrap();
}

#[tokio::test]
async fn invalid_speed_rejected() {
    let mock = mock_wk(23);
    let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
        .build_with_port(mock)
        .await
        .unwrap();

    let result = keyer.set_speed(3).await;
    assert!(result.is_err());

    let result = keyer.set_speed(100).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn invalid_text_rejected() {
    let mock = mock_wk(23);
    let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
        .build_with_port(mock)
        .await
        .unwrap();

    let result = keyer.send_message("CQ~TEST").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn builder_with_all_options() {
    let mock = mock_wk(23);
    let keyer = WinKeyerBuilder::new("/dev/ttyUSB0")
        .speed(30)
        .paddle_mode(PaddleMode::IambicA)
        .contest_spacing(true)
        .auto_space(true)
        .swap_paddles(false)
        .sidetone(7)
        .weight(55)
        .ptt_lead_in_ms(50)
        .ptt_tail_ms(40)
        .min_wpm(15)
        .wpm_range(30)
        .farnsworth(10)
        .dit_dah_ratio(50)
        .prefer_wk3(false)
        .build_with_port(mock.clone())
        .await
        .unwrap();

    assert_eq!(keyer.get_speed().await.unwrap(), 30);
    keyer.close().await.unwrap();
}
