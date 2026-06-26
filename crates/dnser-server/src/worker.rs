use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use dnser_cache::Cache;
use dnser_proto::MAX_UDP_SIZE;
use dnser_resolver::Resolver;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};
use tokio::task::JoinSet;
use tracing::{debug, warn};

use crate::error::QueryError;
use crate::handler::{build_truncated, process_query, query_udp_limit};

pub(crate) struct Worker {
    id: usize,
    socket: Arc<UdpSocket>,
    resolver: Arc<Resolver>,
    cache: Arc<Cache>,
    shutdown: watch::Receiver<bool>,
    drain_timeout: Duration,
    inflight: Arc<Semaphore>,
}

impl Worker {
    pub(crate) fn bind(
        id: usize,
        addr: SocketAddr,
        resolver: Arc<Resolver>,
        cache: Arc<Cache>,
        shutdown: watch::Receiver<bool>,
        drain_timeout: Duration,
        inflight: Arc<Semaphore>,
    ) -> Result<Self, std::io::Error> {
        let domain = Domain::for_address(addr);
        let sock = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;
        sock.set_reuse_port(true)?;
        sock.set_nonblocking(true)?;
        sock.bind(&addr.into())?;
        let socket = Arc::new(UdpSocket::from_std(sock.into())?);
        Ok(Self {
            id,
            socket,
            resolver,
            cache,
            shutdown,
            drain_timeout,
            inflight,
        })
    }

    pub(crate) fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    pub(crate) async fn run(mut self) {
        let mut buf = [0u8; MAX_UDP_SIZE];
        let mut queries: JoinSet<()> = JoinSet::new();

        loop {
            tokio::select! {
                result = self.socket.recv_from(&mut buf) => {
                    match result {
                        Ok((len, peer)) => {
                            let permit = match Arc::clone(&self.inflight).try_acquire_owned() {
                                Ok(p) => p,
                                Err(_) => {
                                    warn!(worker = self.id, peer = %peer, "udp inflight limit reached, dropping");
                                    continue;
                                }
                            };
                            let data = Bytes::copy_from_slice(&buf[..len]);
                            let socket = Arc::clone(&self.socket);
                            let resolver = Arc::clone(&self.resolver);
                            let cache = Arc::clone(&self.cache);
                            queries.spawn(async move {
                                if let Err(e) = handle_query(&socket, &resolver, &cache, data, peer, permit).await {
                                    warn!(peer = %peer, err = %e, "query error");
                                }
                            });
                        }
                        Err(e) => warn!(worker = self.id, err = %e, "recv error"),
                    }
                }
                Some(res) = queries.join_next(), if !queries.is_empty() => {
                    if let Err(e) = res {
                        warn!(worker = self.id, err = %e, "query task panicked");
                    }
                }
                _ = self.shutdown.changed() => break,
            }
        }

        let remaining = queries.len();
        if remaining > 0 {
            let drained = tokio::time::timeout(self.drain_timeout, async {
                while queries.join_next().await.is_some() {}
            })
            .await;

            if drained.is_err() {
                warn!(
                    worker = self.id,
                    remaining, "drain timeout; aborting in-flight queries"
                );
                queries.abort_all();
                while queries.join_next().await.is_some() {}
            }
        }
    }
}

async fn handle_query(
    socket: &UdpSocket,
    resolver: &Resolver,
    cache: &Cache,
    data: Bytes,
    peer: SocketAddr,
    _permit: OwnedSemaphorePermit,
) -> Result<(), QueryError> {
    let query = dnser_proto::Message::parse(data)?;
    debug!(id = query.header.id, peer = %peer, "udp query");

    let udp_limit = query_udp_limit(&query);
    let response = process_query(resolver, cache, &query).await;
    let bytes = response.to_bytes()?;

    if bytes.len() > udp_limit {
        debug!(id = query.header.id, "response truncated for udp");
        socket
            .send_to(&build_truncated(&query).to_bytes()?, peer)
            .await?;
    } else {
        socket.send_to(&bytes, peer).await?;
    }

    Ok(())
}
