use std::net::SocketAddr;
use std::time::Duration;

use dnser_config::{CacheConfig, Config, ResolverConfig, ServerConfig};
use dnser_proto::{Class, Header, Message, Question, RData, RecordType, ResourceRecord};
use dnser_server::ServerHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};

// ── helpers ──────────────────────────────────────────────────────────────────

/// Spins up a mock UDP upstream. `respond` maps raw query bytes to raw response
/// bytes; returning an empty vec suppresses the reply.
async fn mock_upstream(respond: impl Fn(&[u8]) -> Vec<u8> + Send + 'static) -> SocketAddr {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap();
    tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        while let Ok((n, peer)) = socket.recv_from(&mut buf).await {
            let resp = respond(&buf[..n]);
            if !resp.is_empty() {
                let _ = socket.send_to(&resp, peer).await;
            }
        }
    });
    addr
}

/// Starts the server against the given upstream and returns its handle.
async fn start_server(upstream: SocketAddr) -> ServerHandle {
    start_server_with(upstream, 10).await
}

async fn start_server_with(upstream: SocketAddr, tcp_idle_timeout_secs: u64) -> ServerHandle {
    let config = Config {
        server: ServerConfig {
            listen: "127.0.0.1:0".parse().unwrap(),
            workers: 1,
            tcp_idle_timeout_secs,
            ..Default::default()
        },
        resolver: ResolverConfig {
            upstreams: vec![upstream],
            timeout_ms: 500,
        },
        cache: CacheConfig {
            max_entries: 100,
            reaper_interval_secs: 60,
        },
        ..Default::default()
    };
    dnser_server::start(config).await.unwrap()
}

fn make_query(name: &str, qtype: RecordType) -> Message {
    Message {
        header: Header {
            id: 1234,
            flags: Header::RD,
            qd_count: 1,
            ..Default::default()
        },
        questions: vec![Question {
            name: name.to_string(),
            qtype,
            qclass: Class::IN,
        }],
        ..Default::default()
    }
}

/// Mock upstream: echoes the query back as a minimal valid response.
fn echo_response(query_bytes: &[u8]) -> Vec<u8> {
    let query = Message::try_from(query_bytes).unwrap();
    Message {
        header: Header {
            id: query.header.id,
            flags: Header::QR | Header::RA | (query.header.flags & Header::RD),
            qd_count: query.header.qd_count,
            ..Default::default()
        },
        questions: query.questions,
        ..Default::default()
    }
    .to_bytes()
    .unwrap()
    .to_vec()
}

/// Mock upstream: returns 50 A records whose combined wire size exceeds 512 bytes.
/// (header 12 + question ~17 + 50 × 16-byte A records = ~829 bytes)
fn large_response(query_bytes: &[u8]) -> Vec<u8> {
    let query = Message::try_from(query_bytes).unwrap();
    let name = query
        .questions
        .first()
        .map(|q| q.name.clone())
        .unwrap_or_default();
    let answers: Vec<ResourceRecord> = (0u8..50)
        .map(|i| ResourceRecord {
            name: name.clone(),
            class: Class::IN,
            ttl: 300,
            rdata: RData::A(std::net::Ipv4Addr::new(10, 0, 0, i)),
        })
        .collect();
    let an_count = answers.len() as u16;
    Message {
        header: Header {
            id: query.header.id,
            flags: Header::QR | Header::RA,
            qd_count: 1,
            an_count,
            ..Default::default()
        },
        questions: query.questions,
        answers,
        ..Default::default()
    }
    .to_bytes()
    .unwrap()
    .to_vec()
}

// ── TCP helpers ───────────────────────────────────────────────────────────────

async fn write_framed(stream: &mut TcpStream, msg: &Message) {
    let bytes = msg.to_bytes().unwrap();
    stream
        .write_all(&(bytes.len() as u16).to_be_bytes())
        .await
        .unwrap();
    stream.write_all(&bytes).await.unwrap();
}

async fn read_framed(stream: &mut TcpStream) -> Message {
    let mut len_buf = [0u8; 2];
    stream.read_exact(&mut len_buf).await.unwrap();
    let len = u16::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await.unwrap();
    Message::try_from(buf.as_slice()).unwrap()
}

async fn tcp_query(addr: SocketAddr, query: &Message) -> Message {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    write_framed(&mut stream, query).await;
    read_framed(&mut stream).await
}

// ── TCP tests ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn tcp_single_query() {
    let upstream = mock_upstream(echo_response).await;
    let server = start_server(upstream).await;

    let query = make_query("example.com", RecordType::A);
    let response = tcp_query(server.tcp_addr, &query).await;

    assert!(response.header.is_response());
    assert_eq!(response.header.id, query.header.id);
    assert!(!response.header.is_truncated());

    server.shutdown().await;
}

#[tokio::test]
async fn tcp_pipelining() {
    let upstream = mock_upstream(echo_response).await;
    let server = start_server(upstream).await;

    let mut stream = TcpStream::connect(server.tcp_addr).await.unwrap();
    let q1 = make_query("one.example.com", RecordType::A);
    let q2 = make_query("two.example.com", RecordType::AAAA);

    // Send both queries back-to-back before reading any response.
    write_framed(&mut stream, &q1).await;
    write_framed(&mut stream, &q2).await;

    let r1 = read_framed(&mut stream).await;
    let r2 = read_framed(&mut stream).await;

    assert_eq!(r1.header.id, q1.header.id);
    assert_eq!(r2.header.id, q2.header.id);

    server.shutdown().await;
}

#[tokio::test]
async fn tcp_idle_timeout_closes_connection() {
    let upstream = mock_upstream(echo_response).await;
    // Very short idle timeout so the test doesn't take long.
    let server = start_server_with(upstream, 1).await;

    let mut stream = TcpStream::connect(server.tcp_addr).await.unwrap();

    // Wait longer than the idle timeout; the server should close the connection.
    tokio::time::sleep(Duration::from_millis(1500)).await;

    let mut buf = [0u8; 1];
    let n = stream.read(&mut buf).await.unwrap();
    assert_eq!(n, 0, "expected EOF after idle timeout");

    server.shutdown().await;
}

#[tokio::test]
async fn tcp_zero_length_message_closes_connection() {
    let upstream = mock_upstream(echo_response).await;
    let server = start_server(upstream).await;

    let mut stream = TcpStream::connect(server.tcp_addr).await.unwrap();
    // Send a zero-length framing prefix.
    stream.write_all(&[0u8, 0u8]).await.unwrap();

    // Server should close the connection gracefully.
    let mut buf = [0u8; 1];
    let n = stream.read(&mut buf).await.unwrap();
    assert_eq!(n, 0, "expected EOF after zero-length message");

    // Server must still accept new connections.
    assert!(TcpStream::connect(server.tcp_addr).await.is_ok());

    server.shutdown().await;
}

#[tokio::test]
async fn tcp_client_disconnect_mid_message_does_not_crash_server() {
    let upstream = mock_upstream(echo_response).await;
    let server = start_server(upstream).await;

    {
        let mut stream = TcpStream::connect(server.tcp_addr).await.unwrap();
        // Advertise a 50-byte body but then drop the connection without sending it.
        stream.write_all(&[0u8, 50u8]).await.unwrap();
    } // stream dropped here

    // Give the server a moment to handle the disconnect.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Server must still be alive.
    assert!(TcpStream::connect(server.tcp_addr).await.is_ok());

    server.shutdown().await;
}

// ── UDP truncation tests ──────────────────────────────────────────────────────

#[tokio::test]
async fn udp_large_response_sets_tc_bit() {
    let upstream = mock_upstream(large_response).await;
    let server = start_server(upstream).await;

    let query = make_query("example.com", RecordType::A);
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket
        .send_to(&query.to_bytes().unwrap(), server.udp_addr)
        .await
        .unwrap();

    let mut buf = [0u8; 4096];
    let (n, _) = socket.recv_from(&mut buf).await.unwrap();
    let response = Message::try_from(&buf[..n]).unwrap();

    assert!(response.header.is_response());
    assert!(response.header.is_truncated());
    assert_eq!(response.header.id, query.header.id);
    assert!(response.answers.is_empty());

    server.shutdown().await;
}

#[tokio::test]
async fn tcp_serves_full_response_after_udp_truncation() {
    let upstream = mock_upstream(large_response).await;
    let server = start_server(upstream).await;

    let query = make_query("example.com", RecordType::A);

    // Step 1: UDP returns TC=1.
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket
        .send_to(&query.to_bytes().unwrap(), server.udp_addr)
        .await
        .unwrap();
    let mut buf = [0u8; 4096];
    let (n, _) = socket.recv_from(&mut buf).await.unwrap();
    let udp_resp = Message::try_from(&buf[..n]).unwrap();
    assert!(udp_resp.header.is_truncated());

    // Step 2: TCP returns the full answer.
    let tcp_resp = tcp_query(server.tcp_addr, &query).await;
    assert!(!tcp_resp.header.is_truncated());
    assert!(!tcp_resp.answers.is_empty());

    server.shutdown().await;
}

// ── Cache tests ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn tcp_cache_hit_returns_valid_response() {
    let upstream = mock_upstream(echo_response).await;
    let server = start_server(upstream).await;

    let query = make_query("example.com", RecordType::A);

    // First TCP query populates the cache.
    let r1 = tcp_query(server.tcp_addr, &query).await;
    assert!(r1.header.is_response());

    // Second TCP query should hit the cache (same valid response).
    let r2 = tcp_query(server.tcp_addr, &query).await;
    assert!(r2.header.is_response());
    assert_eq!(r2.header.id, query.header.id);

    server.shutdown().await;
}
