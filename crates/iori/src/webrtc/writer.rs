use std::io::{self, Write};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::AsyncWrite;
use tokio::sync::mpsc;

pub struct AsyncBufferWriter {
    buffer: Vec<u8>,
    threshold: usize,
    sender: mpsc::Sender<Vec<u8>>,
}

impl AsyncBufferWriter {
    pub fn new(threshold: usize, sender: mpsc::Sender<Vec<u8>>) -> Self {
        AsyncBufferWriter {
            // Buffer size = threshold + 1KiB
            buffer: Vec::with_capacity(threshold.saturating_add(1024 * 1024)),
            threshold,
            sender,
        }
    }
}

impl Write for AsyncBufferWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);

        loop {
            if self.buffer.len() < self.threshold {
                break;
            }

            let chunk_to_send = self.buffer.drain(0..self.threshold).collect::<Vec<u8>>();

            if let Err(mpsc::error::TrySendError::Closed(_failed_chunk)) =
                self.sender.try_send(chunk_to_send)
            {
                // Channel closed, return a BrokenPipe error
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "mpsc channel closed during try_send in poll_write",
                ));
            }
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        loop {
            if self.buffer.is_empty() {
                return Ok(());
            }

            let chunk_to_send = self.buffer.drain(..).collect::<Vec<u8>>();

            if let Err(mpsc::error::TrySendError::Closed(_failed_chunk)) =
                self.sender.try_send(chunk_to_send)
            {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "mpsc channel closed during try_send in poll_flush",
                ));
            }
        }
    }
}

impl AsyncWrite for AsyncBufferWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let this = self.as_mut().get_mut();

        this.buffer.extend_from_slice(buf);

        loop {
            if this.buffer.len() < this.threshold {
                break;
            }

            let chunk_to_send = this.buffer.drain(0..this.threshold).collect::<Vec<u8>>();

            if let Err(mpsc::error::TrySendError::Closed(_failed_chunk)) =
                this.sender.try_send(chunk_to_send)
            {
                // Channel closed, return a BrokenPipe error
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "mpsc channel closed during try_send in poll_write",
                )));
            }
        }

        // Successfully wrote all buf data to internal buffer.
        // Return the bytes written (buf.length)
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        let this = self.as_mut().get_mut();

        // Try to flush the buffer
        loop {
            if this.buffer.is_empty() {
                return Poll::Ready(Ok(()));
            }

            let chunk_to_send = this.buffer.drain(..).collect::<Vec<u8>>();

            if let Err(mpsc::error::TrySendError::Closed(_failed_chunk)) =
                this.sender.try_send(chunk_to_send)
            {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "mpsc channel closed during try_send in poll_flush",
                )));
            }
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.poll_flush(cx)
    }
}
