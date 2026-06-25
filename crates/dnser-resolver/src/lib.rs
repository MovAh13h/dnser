mod error;

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
use tokio::net::UdpSocket;
use tokio::sync::oneshot;
use tokio::task::AbortHandle;
use tracing::{error, warn};

struct InFlight {
    map: HashMap<u16, oneshot::Sender<Bytes>>,
    next_id: u16,
}

struct UpstreamSocket {
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
            next_id: 0,
        }));

        let recv_abort = tokio::spawn(recv_loop(
            Arc::clone(&socket),
            Arc::clone(&in_flight),
        ))
        .abort_handle();

        Ok(Self {
            socket,
            in_flight,
            recv_abort,
        })
    }

    async fn query(&self, query: &Message, timeout: Duration) -> Result<Message, ResolveError> {
        let original_id = query.header.id;

        // Serialize once, then patch the ID and RD bit in-place — no Message::clone() needed.
        let mut bytes = query.to_bytes()?.to_vec();

        let (tx, rx) = oneshot::channel();
        let assigned_id = self.allocate_and_insert(tx)?;
        let mut guard = InFlightGuard { in_flight: Arc::clone(&self.in_flight), id: Some(assigned_id) };

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
                let mut msg = Message::parse(response_bytes)?;
                if !msg.header.is_response() || msg.questions != query.questions {
                    return Err(ResolveError::InvalidResponse);
                }
                msg.header.id = original_id;
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

    fn allocate_and_insert(&self, tx: oneshot::Sender<Bytes>) -> Result<u16, ResolveError> {
        let mut state = self.in_flight.lock().unwrap();
        let mut tx = Some(tx);
        let start = state.next_id;
        loop {
            let id = state.next_id;
            state.next_id = state.next_id.wrapping_add(1);
            if let Entry::Vacant(e) = state.map.entry(id) {
                e.insert(tx.take().unwrap());
                return Ok(id);
            }
            if state.next_id == start {
                return Err(ResolveError::IdSpaceExhausted);
            }
        }
    }
}

impl Drop for UpstreamSocket {
    fn drop(&mut self) {
        self.recv_abort.abort();
    }
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
            warn!(id, "recv_loop: no in-flight query for ID (late or spurious response)");
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

        let mut futs: FuturesUnordered<_> = self.upstreams
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
