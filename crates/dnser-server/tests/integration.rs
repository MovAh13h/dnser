use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::time::Duration;

use dnser_config::{CacheConfig, Config, ResolverConfig, ServerConfig};
use dnser_net::{read_framed, write_framed};
use dnser_proto::{Message, RData, RecordType, ResourceRecord};
use dnser_server::ServerHandle;
use dnser_testing::{
    make_edns_query, make_query, mocks, spawn_udp_responder as mock_upstream,
    spawn_udp_responder_counted as mock_upstream_counted, tcp_query, udp_query,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

// ── helpers ──────────────────────────────────────────────────────────────────

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

// ── Listener tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn server_binds_ipv6_listen_address() {
    let upstream = mock_upstream(mocks::echo).await;
    let config = Config {
        server: ServerConfig {
            listen: "[::1]:0".parse().unwrap(),
            workers: 1,
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
    let server = dnser_server::start(config).await.unwrap();

    assert!(
        server.udp_addr.is_ipv6(),
        "expected IPv6 UDP bind, got {}",
        server.udp_addr
    );
    assert!(
        server.tcp_addr.is_ipv6(),
        "expected IPv6 TCP bind, got {}",
        server.tcp_addr
    );

    // Round-trip a query to prove the IPv6 socket actually accepts traffic.
    let query = make_query("example.com", RecordType::A);
    let response = udp_query(server.udp_addr, &query).await;
    assert!(response.header.is_response());

    server.shutdown().await;
}

// ── TCP tests ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn tcp_single_query() {
    let upstream = mock_upstream(mocks::echo).await;
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
    let upstream = mock_upstream(mocks::echo).await;
    let server = start_server(upstream).await;

    let mut stream = TcpStream::connect(server.tcp_addr).await.unwrap();
    let q1 = make_query("one.example.com", RecordType::A);
    let q2 = make_query("two.example.com", RecordType::AAAA);

    // Send both queries back-to-back before reading any response.
    write_framed(&mut stream, &q1.to_bytes().unwrap()).await.unwrap();
    write_framed(&mut stream, &q2.to_bytes().unwrap()).await.unwrap();

    let r1 = Message::parse(read_framed(&mut stream).await.unwrap().unwrap()).unwrap();
    let r2 = Message::parse(read_framed(&mut stream).await.unwrap().unwrap()).unwrap();

    assert_eq!(r1.header.id, q1.header.id);
    assert_eq!(r2.header.id, q2.header.id);

    server.shutdown().await;
}

#[tokio::test]
async fn tcp_idle_timeout_closes_connection() {
    let upstream = mock_upstream(mocks::echo).await;
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
    let upstream = mock_upstream(mocks::echo).await;
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
    let upstream = mock_upstream(mocks::echo).await;
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
    // 50 A records is enough to bust the 512-byte UDP limit.
    let upstream = mock_upstream(mocks::many_a_records(50)).await;
    let server = start_server(upstream).await;

    let query = make_query("example.com", RecordType::A);
    let response = udp_query(server.udp_addr, &query).await;

    assert!(response.header.is_response());
    assert!(response.header.is_truncated());
    assert_eq!(response.header.id, query.header.id);
    assert!(response.answers.is_empty());

    server.shutdown().await;
}

#[tokio::test]
async fn tcp_serves_full_response_after_udp_truncation() {
    let upstream = mock_upstream(mocks::many_a_records(50)).await;
    let server = start_server(upstream).await;

    let query = make_query("example.com", RecordType::A);

    // Step 1: UDP returns TC=1.
    let udp_resp = udp_query(server.udp_addr, &query).await;
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
    let upstream = mock_upstream(mocks::echo).await;
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

#[tokio::test]
async fn udp_edns_query_returns_opt_record() {
    let upstream = mock_upstream(mocks::echo).await;
    let server = start_server(upstream).await;

    let query = make_edns_query("example.com", RecordType::A, 4096);
    let response = udp_query(server.udp_addr, &query).await;

    assert!(response.header.is_response());
    let opt = response.opt().expect("response must contain OPT record");
    assert_eq!(opt.edns_udp_size(), Some(4096));

    server.shutdown().await;
}

#[tokio::test]
async fn udp_non_edns_query_returns_no_opt_record() {
    let upstream = mock_upstream(mocks::echo).await;
    let server = start_server(upstream).await;

    let query = make_query("example.com", RecordType::A);
    let response = udp_query(server.udp_addr, &query).await;

    assert!(response.header.is_response());
    assert!(
        response.opt().is_none(),
        "non-EDNS query must not get OPT record"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn tcp_edns_query_returns_opt_record() {
    let upstream = mock_upstream(mocks::echo).await;
    let server = start_server(upstream).await;

    let query = make_edns_query("example.com", RecordType::A, 4096);
    let response = tcp_query(server.tcp_addr, &query).await;

    assert!(response.header.is_response());
    assert!(
        response.opt().is_some(),
        "TCP response must contain OPT record"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn udp_edns_large_payload_not_truncated() {
    // Client advertises 4096-byte payload, so a ~829-byte response
    // should be delivered in full over UDP without TC=1.
    let upstream = mock_upstream(mocks::many_a_records(50)).await;
    let server = start_server(upstream).await;

    let query = make_edns_query("example.com", RecordType::A, 4096);
    let response = udp_query(server.udp_addr, &query).await;

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
    let upstream = mock_upstream(mocks::echo).await;
    let server = start_server(upstream).await;

    // Build a query advertising EDNS version 1 (unsupported).
    let mut query = make_edns_query("example.com", RecordType::A, 4096);
    query.additional.last_mut().unwrap().set_edns_version(1);

    let response = udp_query(server.udp_addr, &query).await;

    assert!(response.header.is_response());
    // BADVERS: header RCODE=0, extended RCODE in OPT TTL byte 0 = 1.
    assert_eq!(response.header.rcode(), Ok(dnser_proto::Rcode::NoError));
    let opt = response.opt().expect("BADVERS response must contain OPT");
    assert_eq!(opt.edns_extended_rcode(), Some(1));

    server.shutdown().await;
}

#[tokio::test]
async fn edns_options_are_not_forwarded_server_opts_empty() {
    // A query carrying a client subnet option should receive a server OPT
    // with no options (we don't implement ECS or option forwarding).
    let upstream = mock_upstream(mocks::echo).await;
    let server = start_server(upstream).await;

    let mut query = make_query("example.com", RecordType::A);
    let mut opt = ResourceRecord::edns_opt(4096);
    opt.rdata = RData::OPT(vec![dnser_proto::EdnsOption {
        code: 8, // ECS
        data: bytes::Bytes::from_static(&[0, 1, 24, 0, 192, 168, 1, 0]),
    }]);
    query.additional.push(opt);
    query.header.ar_count = 1;

    let response = udp_query(server.udp_addr, &query).await;

    assert!(response.header.is_response());
    let opt = response.opt().expect("response must contain OPT");
    assert!(
        matches!(&opt.rdata, RData::OPT(opts) if opts.is_empty()),
        "server OPT must carry no options"
    );

    server.shutdown().await;
}

// ── Negative cache tests ──────────────────────────────────────────────────────

#[tokio::test]
async fn nxdomain_response_is_cached() {
    let (upstream, hits) = mock_upstream_counted(mocks::nxdomain).await;
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
    let (upstream, hits) = mock_upstream_counted(mocks::nodata).await;
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
    let (upstream, _) = mock_upstream_counted(mocks::nxdomain).await;
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
    let (upstream, hits) = mock_upstream_counted(mocks::nxdomain).await;
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
