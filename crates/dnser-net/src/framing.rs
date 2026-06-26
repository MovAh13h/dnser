//! DNS-over-TCP message framing (RFC 1035 §4.2.2).
//!
//! Every DNS message sent over TCP is prefixed with a 2-byte big-endian
//! length. These helpers are generic over `AsyncRead`/`AsyncWrite` so the
//! same primitives serve the server's accept loop, one-shot client queries
//! from the resolver, and the integration-test mocks.

use std::io;

use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Read the next length-prefixed DNS message from `stream`.
///
/// Returns:
/// - `Ok(Some(bytes))` — a complete message body.
/// - `Ok(None)` — the peer closed the connection cleanly at a message
///   boundary, or it sent a zero-length frame (treated as a graceful close
///   to match common DNS-server behaviour).
/// - `Err(e)` — any other I/O error, including a half-read frame
///   (`UnexpectedEof` mid-length-prefix or mid-body).
///
/// Callers wanting an idle timeout should wrap this in
/// [`tokio::time::timeout`].
pub async fn read_framed<S>(stream: &mut S) -> io::Result<Option<Bytes>>
where
    S: AsyncRead + Unpin,
{
    let mut len_buf = [0u8; 2];
    match stream.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }

    let len = u16::from_be_bytes(len_buf) as usize;
    if len == 0 {
        return Ok(None);
    }

    let mut body = vec![0u8; len];
    match stream.read_exact(&mut body).await {
        Ok(_) => Ok(Some(Bytes::from(body))),
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
        Err(e) => Err(e),
    }
}

/// Write a length-prefixed DNS message to `stream`.
///
/// Fails with `InvalidData` if the message exceeds 65535 bytes (the maximum
/// value of the 16-bit length prefix). Real DNS messages are always well
/// under this limit.
pub async fn write_framed<S>(stream: &mut S, msg: &[u8]) -> io::Result<()>
where
    S: AsyncWrite + Unpin,
{
    let len = u16::try_from(msg.len()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "DNS-over-TCP message exceeds 65535 bytes",
        )
    })?;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(msg).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn roundtrip_short_message() {
        let (mut a, mut b) = duplex(64);
        write_framed(&mut a, b"hi").await.unwrap();
        let got = read_framed(&mut b).await.unwrap().unwrap();
        assert_eq!(&got[..], b"hi");
    }

    #[tokio::test]
    async fn roundtrip_two_messages_back_to_back() {
        let (mut a, mut b) = duplex(64);
        write_framed(&mut a, b"one").await.unwrap();
        write_framed(&mut a, b"two").await.unwrap();
        assert_eq!(&read_framed(&mut b).await.unwrap().unwrap()[..], b"one");
        assert_eq!(&read_framed(&mut b).await.unwrap().unwrap()[..], b"two");
    }

    #[tokio::test]
    async fn clean_eof_returns_none() {
        let (a, mut b) = duplex(64);
        drop(a);
        assert!(read_framed(&mut b).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn zero_length_frame_returns_none() {
        let (mut a, mut b) = duplex(64);
        a.write_all(&[0u8, 0u8]).await.unwrap();
        assert!(read_framed(&mut b).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn half_read_length_prefix_is_eof() {
        let (mut a, mut b) = duplex(64);
        a.write_all(&[0u8]).await.unwrap();
        drop(a);
        // Only got 1 of 2 length bytes before EOF — surfaced as Ok(None).
        assert!(read_framed(&mut b).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn truncated_body_returns_none() {
        let (mut a, mut b) = duplex(64);
        // Claim a 10-byte body but send only 5.
        a.write_all(&10u16.to_be_bytes()).await.unwrap();
        a.write_all(b"short").await.unwrap();
        drop(a);
        assert!(read_framed(&mut b).await.unwrap().is_none());
    }
}
