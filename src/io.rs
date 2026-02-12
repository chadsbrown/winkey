//! IO task: single tokio task owns the serial port, biased select loop.
//!
//! Two priority channels (RT for abort/tune/PTT/speed/close, BG for text/config)
//! ensure time-critical operations preempt queued text.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, trace, warn};

use crate::error::{Error, Result};
use crate::event::KeyerEvent;
use crate::protocol::response::{self, ResponseByte};

/// A request sent to the IO task via RT or BG channel.
#[derive(Debug)]
pub(crate) enum Request {
    /// Write bytes to the serial port (fire-and-forget with ack).
    Write {
        data: Vec<u8>,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Write bytes and read back a specific number of response bytes.
    WriteAndRead {
        data: Vec<u8>,
        expected: usize,
        reply: oneshot::Sender<Result<Vec<u8>>>,
    },
    /// Shut down the IO task and return.
    Shutdown {
        reply: oneshot::Sender<Result<()>>,
    },
}

/// Handle for communicating with the IO task.
pub(crate) struct IoHandle {
    pub rt_tx: mpsc::Sender<Request>,
    pub bg_tx: mpsc::Sender<Request>,
    pub cancel: CancellationToken,
    pub task: JoinHandle<()>,
    pub xoff: Arc<AtomicBool>,
}

impl IoHandle {
    /// Send a command via the real-time (priority) channel.
    pub async fn rt_command(&self, data: Vec<u8>) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.rt_tx
            .send(Request::Write {
                data,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Error::NotConnected)?;

        match tokio::time::timeout(std::time::Duration::from_secs(5), reply_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(Error::NotConnected),
            Err(_) => Err(Error::Timeout),
        }
    }

    /// Send a command via the background channel.
    pub async fn bg_command(&self, data: Vec<u8>) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.bg_tx
            .send(Request::Write {
                data,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Error::NotConnected)?;

        match tokio::time::timeout(std::time::Duration::from_secs(5), reply_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(Error::NotConnected),
            Err(_) => Err(Error::Timeout),
        }
    }

    /// Send a command via RT and read back response bytes.
    pub async fn rt_command_read(&self, data: Vec<u8>, expected: usize) -> Result<Vec<u8>> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.rt_tx
            .send(Request::WriteAndRead {
                data,
                expected,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Error::NotConnected)?;

        match tokio::time::timeout(std::time::Duration::from_secs(5), reply_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(Error::NotConnected),
            Err(_) => Err(Error::Timeout),
        }
    }

    /// Request graceful shutdown of the IO task.
    pub async fn shutdown(&self) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        // Try RT channel first, fall back to cancel token
        if self.rt_tx
            .send(Request::Shutdown { reply: reply_tx })
            .await
            .is_err()
        {
            self.cancel.cancel();
            return Ok(());
        }

        match tokio::time::timeout(std::time::Duration::from_secs(2), reply_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => {
                self.cancel.cancel();
                Ok(())
            }
            Err(_) => {
                self.cancel.cancel();
                Ok(())
            }
        }
    }
}

/// Spawn the IO task that owns the serial port.
pub(crate) fn spawn_io_task<P>(
    port: P,
    event_tx: broadcast::Sender<KeyerEvent>,
    min_wpm: u8,
) -> IoHandle
where
    P: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let (rt_tx, rt_rx) = mpsc::channel::<Request>(32);
    let (bg_tx, bg_rx) = mpsc::channel::<Request>(64);
    let cancel = CancellationToken::new();
    let xoff = Arc::new(AtomicBool::new(false));

    let task = tokio::spawn(io_loop(
        port,
        rt_rx,
        bg_rx,
        cancel.clone(),
        event_tx,
        xoff.clone(),
        min_wpm,
    ));

    IoHandle {
        rt_tx,
        bg_tx,
        cancel,
        task,
        xoff,
    }
}

/// The main IO loop. Runs until cancelled or channels close.
async fn io_loop<P>(
    mut port: P,
    mut rt_rx: mpsc::Receiver<Request>,
    mut bg_rx: mpsc::Receiver<Request>,
    cancel: CancellationToken,
    event_tx: broadcast::Sender<KeyerEvent>,
    xoff: Arc<AtomicBool>,
    min_wpm: u8,
) where
    P: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let mut read_buf = [0u8; 64];
    let mut prev_breakin = false;

    debug!("IO task started");

    loop {
        tokio::select! {
            biased;

            // 1. Cancellation token — highest priority
            _ = cancel.cancelled() => {
                debug!("IO task cancelled");
                break;
            }

            // 2. Real-time channel — abort, tune, PTT, speed, close
            req = rt_rx.recv() => {
                match req {
                    Some(Request::Shutdown { reply }) => {
                        debug!("IO task shutdown requested (RT)");
                        let _ = reply.send(Ok(()));
                        return;
                    }
                    Some(req) => {
                        handle_request(req, &mut port, &event_tx).await;
                    }
                    None => {
                        debug!("RT channel closed");
                        break;
                    }
                }
            }

            // 3. Background channel — text, config, prosigns
            req = bg_rx.recv() => {
                match req {
                    Some(Request::Shutdown { reply }) => {
                        debug!("IO task shutdown requested (BG)");
                        let _ = reply.send(Ok(()));
                        return;
                    }
                    Some(req) => {
                        handle_request(req, &mut port, &event_tx).await;
                    }
                    None => {
                        debug!("BG channel closed");
                        break;
                    }
                }
            }

            // 4. Read from serial port — unsolicited data (status, echo, speed pot)
            result = port.read(&mut read_buf) => {
                match result {
                    Ok(0) => {
                        debug!("serial port EOF");
                        let _ = event_tx.send(KeyerEvent::Disconnected);
                        break;
                    }
                    Ok(n) => {
                        for &byte in &read_buf[..n] {
                            process_received_byte(
                                byte,
                                &event_tx,
                                &xoff,
                                &mut prev_breakin,
                                min_wpm,
                            );
                        }
                    }
                    Err(e) => {
                        // WouldBlock is expected for non-blocking reads
                        if e.kind() == std::io::ErrorKind::WouldBlock {
                            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                            continue;
                        }
                        error!("serial read error: {e}");
                        let _ = event_tx.send(KeyerEvent::Disconnected);
                        break;
                    }
                }
            }
        }
    }

    debug!("IO task exiting");
}

/// Handle a single request by writing to the port.
async fn handle_request<P>(
    req: Request,
    port: &mut P,
    event_tx: &broadcast::Sender<KeyerEvent>,
) where
    P: AsyncRead + AsyncWrite + Send + Unpin,
{
    match req {
        Request::Write { data, reply } => {
            trace!("writing {} bytes: {:02X?}", data.len(), data);
            let result = port.write_all(&data).await.map_err(|e| {
                error!("write error: {e}");
                let _ = event_tx.send(KeyerEvent::Disconnected);
                Error::Io(e)
            });
            let _ = reply.send(result);
        }
        Request::WriteAndRead {
            data,
            expected,
            reply,
        } => {
            trace!("write+read {} bytes, expecting {}", data.len(), expected);
            let write_result = port.write_all(&data).await;
            if let Err(e) = write_result {
                error!("write error: {e}");
                let _ = event_tx.send(KeyerEvent::Disconnected);
                let _ = reply.send(Err(Error::Io(e)));
                return;
            }

            let mut response = vec![0u8; expected];
            match tokio::time::timeout(
                std::time::Duration::from_secs(2),
                read_exact_from(port, &mut response),
            )
            .await
            {
                Ok(Ok(())) => {
                    let _ = reply.send(Ok(response));
                }
                Ok(Err(e)) => {
                    error!("read error: {e}");
                    let _ = reply.send(Err(Error::Io(e)));
                }
                Err(_) => {
                    warn!("read timeout waiting for {} bytes", expected);
                    let _ = reply.send(Err(Error::Timeout));
                }
            }
        }
        Request::Shutdown { reply } => {
            // Handled in the main loop, but just in case:
            let _ = reply.send(Ok(()));
        }
    }
}

/// Read exactly `buf.len()` bytes from an async reader.
async fn read_exact_from<P>(port: &mut P, buf: &mut [u8]) -> std::io::Result<()>
where
    P: AsyncRead + Unpin,
{
    let mut offset = 0;
    while offset < buf.len() {
        let n = port.read(&mut buf[offset..]).await?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "port closed during read",
            ));
        }
        offset += n;
    }
    Ok(())
}

/// Process a single received byte from the WinKeyer.
fn process_received_byte(
    byte: u8,
    event_tx: &broadcast::Sender<KeyerEvent>,
    xoff: &AtomicBool,
    prev_breakin: &mut bool,
    min_wpm: u8,
) {
    match response::classify_byte(byte) {
        ResponseByte::Status(status) => {
            // Update XOFF atomic for fast-path checking
            xoff.store(status.xoff, Ordering::Release);

            // Detect breakin edge (0→1 transition)
            if status.breakin && !*prev_breakin {
                let _ = event_tx.send(KeyerEvent::PaddleBreakIn);
            }
            *prev_breakin = status.breakin;

            let _ = event_tx.send(KeyerEvent::StatusChanged(status));
        }
        ResponseByte::SpeedPot { value } => {
            let wpm = min_wpm.saturating_add(value);
            let _ = event_tx.send(KeyerEvent::SpeedPotChanged { wpm });
        }
        ResponseByte::Echo(ch) => {
            let _ = event_tx.send(KeyerEvent::CharacterSent(ch));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockPort;

    #[tokio::test]
    async fn io_task_write_command() {
        let mock = MockPort::new();
        let (event_tx, _rx) = broadcast::channel(16);
        let io = spawn_io_task(mock.clone(), event_tx, 10);

        // Send a command via RT channel
        let result = io.rt_command(vec![0x02, 28]).await;
        assert!(result.is_ok());

        // Verify it was written
        let written = mock.written_data();
        assert_eq!(written, vec![0x02, 28]);

        io.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn io_task_bg_command() {
        let mock = MockPort::new();
        let (event_tx, _rx) = broadcast::channel(16);
        let io = spawn_io_task(mock.clone(), event_tx, 10);

        // Send text via BG channel
        let result = io.bg_command(b"CQ TEST".to_vec()).await;
        assert!(result.is_ok());

        let written = mock.written_data();
        assert_eq!(written, b"CQ TEST");

        io.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn io_task_receives_status() {
        let mock = MockPort::new();
        let (event_tx, mut event_rx) = broadcast::channel(16);

        // Queue a status byte before spawning so the IO task reads it
        mock.queue_read(&[0xC0]); // status: all clear

        let io = spawn_io_task(mock.clone(), event_tx, 10);

        // Wait for the event
        let event = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            event_rx.recv(),
        )
        .await;

        assert!(event.is_ok());
        if let Ok(Ok(KeyerEvent::StatusChanged(status))) = event {
            assert!(!status.xoff);
            assert!(!status.busy);
        } else {
            panic!("expected StatusChanged event, got {:?}", event);
        }

        io.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn io_task_receives_echo() {
        let mock = MockPort::new();
        let (event_tx, mut event_rx) = broadcast::channel(16);

        mock.queue_read(&[b'C', b'Q']);
        let io = spawn_io_task(mock.clone(), event_tx, 10);

        let ev1 = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            event_rx.recv(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(matches!(ev1, KeyerEvent::CharacterSent('C')));

        let ev2 = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            event_rx.recv(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(matches!(ev2, KeyerEvent::CharacterSent('Q')));

        io.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn io_task_speed_pot_event() {
        let mock = MockPort::new();
        let (event_tx, mut event_rx) = broadcast::channel(16);

        // 0x8A = speed pot, value 10, min_wpm=10 → 20 WPM
        mock.queue_read(&[0x8A]);
        let io = spawn_io_task(mock.clone(), event_tx, 10);

        let event = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            event_rx.recv(),
        )
        .await
        .unwrap()
        .unwrap();

        assert!(matches!(event, KeyerEvent::SpeedPotChanged { wpm: 20 }));

        io.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn io_task_xoff_tracking() {
        let mock = MockPort::new();
        let (event_tx, _rx) = broadcast::channel(16);

        // Queue status with XOFF set
        mock.queue_read(&[0xC2]); // xoff=true
        let io = spawn_io_task(mock.clone(), event_tx, 10);

        // Give the IO task time to process
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(io.xoff.load(Ordering::Acquire));

        // Queue status with XOFF clear
        mock.queue_read(&[0xC0]); // xoff=false
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(!io.xoff.load(Ordering::Acquire));

        io.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn io_task_breakin_edge_detection() {
        let mock = MockPort::new();
        let (event_tx, mut event_rx) = broadcast::channel(16);

        // Queue breakin transition: no breakin → breakin
        mock.queue_read(&[0xC0, 0xC4]); // clear, then breakin
        let io = spawn_io_task(mock.clone(), event_tx, 10);

        // First event: StatusChanged (clear)
        let ev1 = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            event_rx.recv(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(matches!(ev1, KeyerEvent::StatusChanged(_)));

        // Second event: PaddleBreakIn (edge detection)
        let ev2 = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            event_rx.recv(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(matches!(ev2, KeyerEvent::PaddleBreakIn));

        // Third event: StatusChanged (with breakin)
        let ev3 = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            event_rx.recv(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(matches!(ev3, KeyerEvent::StatusChanged(s) if s.breakin));

        io.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn io_task_shutdown() {
        let mock = MockPort::new();
        let (event_tx, _rx) = broadcast::channel(16);
        let io = spawn_io_task(mock, event_tx, 10);

        let result = io.shutdown().await;
        assert!(result.is_ok());

        // Task should complete
        tokio::time::timeout(std::time::Duration::from_millis(100), io.task)
            .await
            .expect("task should complete")
            .expect("task should not panic");
    }

    #[tokio::test]
    async fn io_task_cancel() {
        let mock = MockPort::new();
        let (event_tx, _rx) = broadcast::channel(16);
        let io = spawn_io_task(mock, event_tx, 10);

        io.cancel.cancel();

        tokio::time::timeout(std::time::Duration::from_millis(100), io.task)
            .await
            .expect("task should complete")
            .expect("task should not panic");
    }

    #[tokio::test]
    async fn io_task_write_and_read() {
        let mock = MockPort::new();
        let (event_tx, _rx) = broadcast::channel(16);

        // Queue response for echo test
        mock.queue_read(&[0x42]);
        let io = spawn_io_task(mock.clone(), event_tx, 10);

        let result = io
            .rt_command_read(vec![0x00, 0x04, 0x42], 1)
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec![0x42]);

        io.shutdown().await.unwrap();
    }
}
