mod error;
mod tcp;

pub use error::ResolveError;

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use dnser_config::ResolverConfig;
use dnser_proto::{Header, MAX_UDP_SIZE, Message};
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use rand::Rng;
use tokio::net::UdpSocket;
use tokio::sync::oneshot;
use tokio::task::AbortHandle;
use tracing::{debug, error, warn};

/// Number of random allocations we attempt before falling back to a linear
/// scan. With this many tries, collision becomes very unlikely until the
/// in-flight map is several thousand entries deep (birthday paradox).
const RANDOM_ID_ATTEMPTS: usize = 16;

struct InFlight {
    map: HashMap<u16, oneshot::Sender<Bytes>>,
}

struct UpstreamSocket {
    addr: SocketAddr,
    socket: Arc<UdpSocket>,
    in_flight: Arc<Mutex<InFlight>>,
    recv_abort: AbortHandle,
}

// Removes the in-flight ID from the map when dropped. Covers timeout, send errors, and
// async cancellation (when resolve() returns early on the first successful upstream).
struct InFlightGuard {
    in_flight: Arc<Mutex<InFlight>>,
    id: Option<u16>,
}

impl InFlightGuard {
    fn disarm(&mut self) {
        self.id = None;
    }
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if let Some(id) = self.id {
            self.in_flight.lock().unwrap().map.remove(&id);
        }
    }
}

impl UpstreamSocket {
    async fn new(addr: SocketAddr) -> std::io::Result<Self> {
        let socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
        socket.connect(addr).await?;

        let in_flight = Arc::new(Mutex::new(InFlight {
            map: HashMap::new(),
        }));

        let recv_abort =
            tokio::spawn(recv_loop(Arc::clone(&socket), Arc::clone(&in_flight))).abort_handle();

        Ok(Self {
            addr,
            socket,
            in_flight,
            recv_abort,
        })
    }

    async fn query(&self, query: &Message, timeout: Duration) -> Result<Message, ResolveError> {
        let original_id = query.header.id;

        // Serialize once into a mutable buffer, then patch the ID and RD bit in-place.
        let mut bytes = query.to_bytes_mut()?;

        let (tx, rx) = oneshot::channel();
        let assigned_id = self.allocate_and_insert(tx)?;
        let mut guard = InFlightGuard {
            in_flight: Arc::clone(&self.in_flight),
            id: Some(assigned_id),
        };

        bytes[0..2].copy_from_slice(&assigned_id.to_be_bytes());
        bytes[2] |= (Header::RD >> 8) as u8; // RD lives in bit 0 of the flags high byte

        if let Err(e) = self.socket.send(&bytes).await {
            return Err(e.into());
        }

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(response_bytes)) => {
                // recv_loop already removed this entry before calling tx.send(); disarm to
                // prevent a stale remove that could race with a new query reusing the same ID.
                guard.disarm();
                let msg = parse_validated_response(response_bytes, query, original_id)?;
                // RFC 1035 §4.2.1: TC=1 means the answer was truncated to fit UDP.
                // Re-issue over TCP for the full response.
                if msg.header.is_truncated() {
                    debug!(upstream = %self.addr, "udp response truncated, retrying over tcp");
                    return tcp::tcp_query(self.addr, query, timeout).await;
                }
                Ok(msg)
            }
            Ok(Err(_)) => {
                // Sender dropped without sending (upstream socket shut down).
                guard.disarm();
                Err(ResolveError::AllFailed)
            }
            Err(_) => Err(ResolveError::Timeout),
        }
    }

    /// Allocates a fresh in-flight query ID and registers `tx` against it.
    ///
    /// IDs are drawn at random (RFC 5452 §9.2: predictable IDs make off-path
    /// spoofing materially easier). On the rare collision we retry a handful
    /// of times, then fall back to a linear scan from a random starting point
    /// so we still surface [`ResolveError::IdSpaceExhausted`] deterministically
    /// when all 65536 IDs are in use.
    fn allocate_and_insert(&self, tx: oneshot::Sender<Bytes>) -> Result<u16, ResolveError> {
        let mut state = self.in_flight.lock().unwrap();
        let mut tx = Some(tx);
        let mut rng = rand::thread_rng();

        for _ in 0..RANDOM_ID_ATTEMPTS {
            let id: u16 = rng.r#gen();
            if let Entry::Vacant(e) = state.map.entry(id) {
                e.insert(tx.take().unwrap());
                return Ok(id);
            }
        }

        // Fall back to a scan from a random starting point.
        let start: u16 = rng.r#gen();
        for offset in 0..=u16::MAX {
            let id = start.wrapping_add(offset);
            if let Entry::Vacant(e) = state.map.entry(id) {
                e.insert(tx.take().unwrap());
                return Ok(id);
            }
        }

        Err(ResolveError::IdSpaceExhausted)
    }
}

impl Drop for UpstreamSocket {
    fn drop(&mut self) {
        self.recv_abort.abort();
    }
}

/// Parses `bytes` as the response to `query`, rejects it if the QR bit is
/// missing or the question section doesn't match, and restores the original
/// id in place of the on-wire one (which the resolver rewrote before sending).
///
/// Used by both the UDP path in [`UpstreamSocket::query`] and the TCP fallback
/// in [`tcp::tcp_query`].
pub(crate) fn parse_validated_response(
    bytes: Bytes,
    query: &Message,
    original_id: u16,
) -> Result<Message, ResolveError> {
    let mut msg = Message::parse(bytes)?;
    if !msg.header.is_response() || msg.questions != query.questions {
        return Err(ResolveError::InvalidResponse);
    }
    msg.header.id = original_id;
    Ok(msg)
}

async fn recv_loop(socket: Arc<UdpSocket>, in_flight: Arc<Mutex<InFlight>>) {
    let mut buf = [0u8; MAX_UDP_SIZE];
    loop {
        let n = match socket.recv(&mut buf).await {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                // ICMP port-unreachable on a connected UDP socket (Linux surfaces as ECONNREFUSED).
                warn!("upstream ICMP unreachable");
                continue;
            }
            Err(e) => {
                error!(err = %e, "recv_loop: fatal socket error, exiting");
                break;
            }
        };

        if n < 12 {
            warn!(bytes = n, "recv_loop: response too short, skipping");
            continue;
        }

        let id = u16::from_be_bytes([buf[0], buf[1]]);
        let tx = in_flight.lock().unwrap().map.remove(&id);

        if let Some(tx) = tx {
            let _ = tx.send(Bytes::copy_from_slice(&buf[..n]));
        } else {
            warn!(
                id,
                "recv_loop: no in-flight query for ID (late or spurious response)"
            );
        }
    }
}

pub struct Resolver {
    upstreams: Vec<UpstreamSocket>,
    timeout: Duration,
}

impl Resolver {
    pub async fn new(config: ResolverConfig) -> std::io::Result<Self> {
        let timeout = Duration::from_millis(config.timeout_ms);
        let mut upstreams = Vec::with_capacity(config.upstreams.len());
        for addr in config.upstreams {
            upstreams.push(UpstreamSocket::new(addr).await?);
        }
        Ok(Self { upstreams, timeout })
    }

    pub async fn resolve(&self, query: &Message) -> Result<Message, ResolveError> {
        if self.upstreams.is_empty() {
            return Err(ResolveError::NoUpstreams);
        }

        let mut futs: FuturesUnordered<_> = self
            .upstreams
            .iter()
            .map(|u| u.query(query, self.timeout))
            .collect();

        while let Some(result) = futs.next().await {
            match result {
                Ok(msg) => return Ok(msg),
                Err(e) => warn!(err = %e, "upstream failed"),
            }
        }

        Err(ResolveError::AllFailed)
    }
}
