use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use dnser_config::{CacheConfig, Config, ResolverConfig, ServerConfig};
use dnser_proto::{
    Class, EdnsOption, Header, Message, Question, RData, RecordType, ResourceRecord,
};
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
            max_negative_ttl_secs: 3600,
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

fn make_edns_query(name: &str, qtype: RecordType, udp_payload: u16) -> Message {
    let mut q = make_query(name, qtype);
    q.additional.push(ResourceRecord {
        name: String::new(),
        class: Class::from(udp_payload),
        ttl: 0,
        rdata: RData::OPT(Vec::new()),
    });
    q.header.ar_count = 1;
    q
}

/// Spins up a counting mock upstream; returns the socket address and a hit counter.
async fn mock_upstream_counted(
    respond: impl Fn(&[u8]) -> Vec<u8> + Send + 'static,
) -> (SocketAddr, Arc<AtomicUsize>) {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap();
    let counter = Arc::new(AtomicUsize::new(0));
    let counter2 = Arc::clone(&counter);
    tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        while let Ok((n, peer)) = socket.recv_from(&mut buf).await {
            counter2.fetch_add(1, Ordering::SeqCst);
            let resp = respond(&buf[..n]);
            if !resp.is_empty() {
                let _ = socket.send_to(&resp, peer).await;
            }
        }
    });
    (addr, counter)
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

fn soa_record(zone: &str, ttl: u32, minimum: u32) -> ResourceRecord {
    ResourceRecord {
        name: zone.to_string(),
        class: Class::IN,
        ttl,
        rdata: RData::SOA {
            mname: format!("ns1.{zone}"),
            rname: format!("admin.{zone}"),
            serial: 1,
            refresh: 3600,
            retry: 600,
            expire: 86400,
            minimum,
        },
    }
}

/// Mock upstream: returns NXDOMAIN with SOA (TTL 60, minimum 60).
fn nxdomain_response(query_bytes: &[u8]) -> Vec<u8> {
    let query = Message::try_from(query_bytes).unwrap();
    let zone = query
        .questions
        .first()
        .map(|q| q.name.clone())
        .unwrap_or_else(|| "example.com".to_string());
    Message {
        header: Header {
            id: query.header.id,
            flags: Header::QR | Header::RA | (dnser_proto::Rcode::NXDomain as u16),
            qd_count: query.header.qd_count,
            ns_count: 1,
            ..Default::default()
        },
        questions: query.questions,
        authority: vec![soa_record(&zone, 60, 60)],
        ..Default::default()
    }
    .to_bytes()
    .unwrap()
    .to_vec()
}

/// Mock upstream: returns NODATA (NOERROR, empty answers) with SOA (TTL 60, minimum 60).
fn nodata_response(query_bytes: &[u8]) -> Vec<u8> {
    let query = Message::try_from(query_bytes).unwrap();
    let zone = query
        .questions
        .first()
        .map(|q| q.name.clone())
        .unwrap_or_else(|| "example.com".to_string());
    Message {
        header: Header {
            id: query.header.id,
            flags: Header::QR | Header::RA,
            qd_count: query.header.qd_count,
            ns_count: 1,
            ..Default::default()
        },
        questions: query.questions,
        authority: vec![soa_record(&zone, 60, 60)],
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

// ── EDNS(0) tests ─────────────────────────────────────────────────────────────

fn has_opt(msg: &Message) -> bool {
    msg.additional
        .iter()
        .any(|rr| matches!(rr.rdata, RData::OPT(_)))
}

fn opt_udp_size(msg: &Message) -> Option<u16> {
    msg.additional
        .iter()
        .find(|rr| matches!(rr.rdata, RData::OPT(_)))
        .map(|rr| u16::from(rr.class))
}

#[tokio::test]
async fn udp_edns_query_returns_opt_record() {
    let upstream = mock_upstream(echo_response).await;
    let server = start_server(upstream).await;

    let query = make_edns_query("example.com", RecordType::A, 4096);
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket
        .send_to(&query.to_bytes().unwrap(), server.udp_addr)
        .await
        .unwrap();
    let mut buf = [0u8; 4096];
    let (n, _) = socket.recv_from(&mut buf).await.unwrap();
    let response = Message::try_from(&buf[..n]).unwrap();

    assert!(response.header.is_response());
    assert!(has_opt(&response), "response must contain OPT record");
    assert_eq!(opt_udp_size(&response), Some(4096));

    server.shutdown().await;
}

#[tokio::test]
async fn udp_non_edns_query_returns_no_opt_record() {
    let upstream = mock_upstream(echo_response).await;
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
    assert!(
        !has_opt(&response),
        "non-EDNS query must not get OPT record"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn tcp_edns_query_returns_opt_record() {
    let upstream = mock_upstream(echo_response).await;
    let server = start_server(upstream).await;

    let query = make_edns_query("example.com", RecordType::A, 4096);
    let response = tcp_query(server.tcp_addr, &query).await;

    assert!(response.header.is_response());
    assert!(has_opt(&response), "TCP response must contain OPT record");

    server.shutdown().await;
}

#[tokio::test]
async fn udp_edns_large_payload_not_truncated() {
    // Client advertises 4096-byte payload, so the large response (~829 bytes)
    // should be delivered in full over UDP without TC=1.
    let upstream = mock_upstream(large_response).await;
    let server = start_server(upstream).await;

    let query = make_edns_query("example.com", RecordType::A, 4096);
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket
        .send_to(&query.to_bytes().unwrap(), server.udp_addr)
        .await
        .unwrap();
    let mut buf = [0u8; 4096];
    let (n, _) = socket.recv_from(&mut buf).await.unwrap();
    let response = Message::try_from(&buf[..n]).unwrap();

    assert!(response.header.is_response());
    assert!(
        !response.header.is_truncated(),
        "EDNS client should receive full response"
    );
    assert!(!response.answers.is_empty());

    server.shutdown().await;
}

#[tokio::test]
async fn edns_version_mismatch_returns_badvers() {
    let upstream = mock_upstream(echo_response).await;
    let server = start_server(upstream).await;

    // Build a query with OPT version=1 (unsupported).
    let mut query = make_query("example.com", RecordType::A);
    query.additional.push(ResourceRecord {
        name: String::new(),
        class: Class::from(4096u16),
        // TTL layout: ext_rcode(8)|version(8)|flags(16); version=1 → 0x00010000
        ttl: 0x0001_0000,
        rdata: RData::OPT(Vec::new()),
    });
    query.header.ar_count = 1;

    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket
        .send_to(&query.to_bytes().unwrap(), server.udp_addr)
        .await
        .unwrap();
    let mut buf = [0u8; 4096];
    let (n, _) = socket.recv_from(&mut buf).await.unwrap();
    let response = Message::try_from(&buf[..n]).unwrap();

    assert!(response.header.is_response());
    // BADVERS: header RCODE=0, extended RCODE in OPT TTL byte 0 = 1
    assert_eq!(response.header.rcode(), Ok(dnser_proto::Rcode::NoError));
    let opt = response
        .additional
        .iter()
        .find(|rr| matches!(rr.rdata, RData::OPT(_)))
        .expect("BADVERS response must contain OPT");
    assert_eq!(opt.ttl >> 24, 1, "extended RCODE must be BADVERS (1)");

    server.shutdown().await;
}

#[tokio::test]
async fn edns_options_are_not_forwarded_server_opts_empty() {
    // A query carrying a client subnet option should receive a server OPT
    // with no options (we don't implement ECS or option forwarding).
    let upstream = mock_upstream(echo_response).await;
    let server = start_server(upstream).await;

    let mut query = make_query("example.com", RecordType::A);
    query.additional.push(ResourceRecord {
        name: String::new(),
        class: Class::from(4096u16),
        ttl: 0,
        rdata: RData::OPT(vec![EdnsOption {
            code: 8, // ECS
            data: bytes::Bytes::from_static(&[0, 1, 24, 0, 192, 168, 1, 0]),
        }]),
    });
    query.header.ar_count = 1;

    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket
        .send_to(&query.to_bytes().unwrap(), server.udp_addr)
        .await
        .unwrap();
    let mut buf = [0u8; 4096];
    let (n, _) = socket.recv_from(&mut buf).await.unwrap();
    let response = Message::try_from(&buf[..n]).unwrap();

    assert!(response.header.is_response());
    assert!(has_opt(&response));
    let opt = response
        .additional
        .iter()
        .find(|rr| matches!(rr.rdata, RData::OPT(_)))
        .unwrap();
    assert!(
        matches!(&opt.rdata, RData::OPT(opts) if opts.is_empty()),
        "server OPT must carry no options"
    );

    server.shutdown().await;
}

// ── Negative cache tests ──────────────────────────────────────────────────────

async fn udp_query(server_addr: SocketAddr, query: &Message) -> Message {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket
        .send_to(&query.to_bytes().unwrap(), server_addr)
        .await
        .unwrap();
    let mut buf = [0u8; 4096];
    let (n, _) = socket.recv_from(&mut buf).await.unwrap();
    Message::try_from(&buf[..n]).unwrap()
}

#[tokio::test]
async fn nxdomain_response_is_cached() {
    let (upstream, hits) = mock_upstream_counted(nxdomain_response).await;
    let server = start_server(upstream).await;
    let query = make_query("nx.example.com", RecordType::A);

    // First query — hits upstream, gets NXDOMAIN.
    let r1 = udp_query(server.udp_addr, &query).await;
    assert!(r1.header.is_response());
    assert_eq!(
        r1.header.rcode(),
        Ok(dnser_proto::Rcode::NXDomain),
        "first query must return NXDOMAIN"
    );
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    // Second query — must be served from cache, not from upstream.
    let r2 = udp_query(server.udp_addr, &query).await;
    assert_eq!(
        r2.header.rcode(),
        Ok(dnser_proto::Rcode::NXDomain),
        "cached response must still be NXDOMAIN"
    );
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "upstream must not be hit again"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn nodata_response_is_cached() {
    let (upstream, hits) = mock_upstream_counted(nodata_response).await;
    let server = start_server(upstream).await;
    let query = make_query("nodata.example.com", RecordType::AAAA);

    // First query — hits upstream, gets NODATA (NOERROR, empty answers).
    let r1 = udp_query(server.udp_addr, &query).await;
    assert!(r1.header.is_response());
    assert_eq!(r1.header.rcode(), Ok(dnser_proto::Rcode::NoError));
    assert!(
        r1.answers.is_empty(),
        "NODATA response must have no answers"
    );
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    // Second query — must be served from cache.
    let r2 = udp_query(server.udp_addr, &query).await;
    assert_eq!(r2.header.rcode(), Ok(dnser_proto::Rcode::NoError));
    assert!(r2.answers.is_empty());
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "upstream must not be hit again"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn nxdomain_ttl_decrements_across_requests() {
    let (upstream, _) = mock_upstream_counted(nxdomain_response).await;
    let server = start_server(upstream).await;
    let query = make_query("nx2.example.com", RecordType::A);

    let r1 = udp_query(server.udp_addr, &query).await;
    let soa1_ttl = r1
        .authority
        .iter()
        .find(|rr| matches!(rr.rdata, RData::SOA { .. }))
        .map(|rr| rr.ttl)
        .expect("NXDOMAIN response must include SOA");

    // Advance time a bit, then check the TTL decremented.
    tokio::time::sleep(Duration::from_millis(1100)).await;

    let r2 = udp_query(server.udp_addr, &query).await;
    let soa2_ttl = r2
        .authority
        .iter()
        .find(|rr| matches!(rr.rdata, RData::SOA { .. }))
        .map(|rr| rr.ttl)
        .expect("cached NXDOMAIN must include SOA");

    assert!(
        soa2_ttl < soa1_ttl,
        "SOA TTL must decrement on cache hit (was {soa1_ttl}, now {soa2_ttl})"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn tcp_nxdomain_response_is_cached() {
    let (upstream, hits) = mock_upstream_counted(nxdomain_response).await;
    let server = start_server(upstream).await;
    let query = make_query("nx3.example.com", RecordType::A);

    let r1 = tcp_query(server.tcp_addr, &query).await;
    assert_eq!(r1.header.rcode(), Ok(dnser_proto::Rcode::NXDomain));
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    let r2 = tcp_query(server.tcp_addr, &query).await;
    assert_eq!(r2.header.rcode(), Ok(dnser_proto::Rcode::NXDomain));
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "upstream must not be hit again"
    );

    server.shutdown().await;
}
