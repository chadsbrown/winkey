//! Serial port transport and MockPort for testing.

use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Open a serial port for WinKeyer communication.
///
/// Default parameters: 1200 baud, 8N2 (8 data bits, no parity, 2 stop bits).
pub fn open_serial(
    path: &str,
    baud_rate: u32,
) -> crate::Result<tokio_serial::SerialStream> {
    let builder = tokio_serial::new(path, baud_rate)
        .data_bits(tokio_serial::DataBits::Eight)
        .parity(tokio_serial::Parity::None)
        .stop_bits(tokio_serial::StopBits::Two)
        .flow_control(tokio_serial::FlowControl::None);

    let port = tokio_serial::SerialStream::open(&builder).map_err(|e| {
        crate::Error::Transport(format!("failed to open {}: {}", path, e))
    })?;

    Ok(port)
}

// ---------------------------------------------------------------------------
// MockPort for testing
// ---------------------------------------------------------------------------

/// Shared state for the mock port.
struct MockState {
    /// Bytes available for the reader (WK → host direction).
    read_buf: Vec<u8>,
    /// All bytes written by the host (host → WK direction).
    write_log: Vec<u8>,
    /// Whether the port is "closed".
    closed: bool,
    /// Waker to notify when new data is queued.
    read_waker: Option<Waker>,
}

/// A mock serial port implementing `AsyncRead + AsyncWrite` for testing.
///
/// Pre-load response bytes with `queue_read()`, then inspect what was
/// written with `written_data()`. When no data is available, reads
/// properly return `Pending` and wake when `queue_read()` is called.
#[derive(Clone)]
pub struct MockPort {
    state: Arc<Mutex<MockState>>,
}

impl MockPort {
    /// Create a new MockPort with no queued data.
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockState {
                read_buf: Vec::new(),
                write_log: Vec::new(),
                closed: false,
                read_waker: None,
            })),
        }
    }

    /// Queue bytes that will be returned by reads (simulating WK → host).
    /// Wakes any pending readers.
    pub fn queue_read(&self, data: &[u8]) {
        let mut state = self.state.lock().unwrap();
        state.read_buf.extend_from_slice(data);
        if let Some(waker) = state.read_waker.take() {
            waker.wake();
        }
    }

    /// Get all bytes written to the port (host → WK).
    pub fn written_data(&self) -> Vec<u8> {
        self.state.lock().unwrap().write_log.clone()
    }

    /// Check if there are pending read bytes.
    pub fn has_pending_reads(&self) -> bool {
        !self.state.lock().unwrap().read_buf.is_empty()
    }

    /// Mark the port as closed (subsequent reads/writes return error).
    pub fn close(&self) {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        if let Some(waker) = state.read_waker.take() {
            waker.wake();
        }
    }
}

impl Default for MockPort {
    fn default() -> Self {
        Self::new()
    }
}

impl AsyncRead for MockPort {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "mock port closed",
            )));
        }

        if state.read_buf.is_empty() {
            // No data available. Store waker for notification when data arrives.
            state.read_waker = Some(cx.waker().clone());
            return Poll::Pending;
        }

        let n = buf.remaining().min(state.read_buf.len());
        buf.put_slice(&state.read_buf[..n]);
        state.read_buf.drain(..n);
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for MockPort {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "mock port closed",
            )));
        }

        state.write_log.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let state = self.state.lock().unwrap();
        if state.closed {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "mock port closed",
            )));
        }
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        if let Some(waker) = state.read_waker.take() {
            waker.wake();
        }
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn mock_write_and_read_log() {
        let mock = MockPort::new();
        let mut port = mock.clone();

        port.write_all(b"\x00\x02").await.unwrap();
        assert_eq!(mock.written_data(), b"\x00\x02");
    }

    #[tokio::test]
    async fn mock_read_queued_data() {
        let mock = MockPort::new();
        mock.queue_read(&[23]); // version byte

        let mut port = mock.clone();
        let mut buf = [0u8; 1];
        port.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf[0], 23);
    }

    #[tokio::test]
    async fn mock_read_write_sequence() {
        let mock = MockPort::new();
        mock.queue_read(&[0xC0]); // status byte

        let mut port = mock.clone();

        // Write a command
        port.write_all(&[0x00, 0x02]).await.unwrap();

        // Read the response
        let mut buf = [0u8; 1];
        port.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf[0], 0xC0);

        // Check write log
        assert_eq!(mock.written_data(), vec![0x00, 0x02]);
    }

    #[tokio::test]
    async fn mock_closed_port() {
        let mock = MockPort::new();
        mock.close();

        let mut port = mock.clone();
        let result = port.write_all(b"\x00\x02").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn mock_delayed_read() {
        let mock = MockPort::new();
        let mock_clone = mock.clone();

        // Queue data after a delay
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            mock_clone.queue_read(&[42]);
        });

        let mut port = mock.clone();
        let mut buf = [0u8; 1];
        port.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf[0], 42);
    }

    #[tokio::test]
    async fn mock_read_timeout_when_empty() {
        let mock = MockPort::new();
        let mut port = mock.clone();
        let mut buf = [0u8; 1];

        // Should timeout since no data is queued
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            port.read(&mut buf),
        )
        .await;

        assert!(result.is_err()); // Timeout
    }
}
