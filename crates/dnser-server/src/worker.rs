use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tokio::sync::watch;
use tokio::task::JoinSet;
use tracing::{debug, warn};

use crate::error::QueryError;
use crate::handler::build_response;

const MAX_UDP_SIZE: usize = 512;

pub(crate) struct Worker {
    id: usize,
    socket: Arc<UdpSocket>,
    shutdown: watch::Receiver<bool>,
    drain_timeout: Duration,
}

impl Worker {
    pub(crate) fn bind(
        id: usize,
        addr: SocketAddr,
        shutdown: watch::Receiver<bool>,
        drain_timeout: Duration,
    ) -> Result<Self, std::io::Error> {
        let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        sock.set_reuse_port(true)?;
        sock.set_nonblocking(true)?;
        sock.bind(&addr.into())?;
        let socket = Arc::new(UdpSocket::from_std(sock.into())?);
        Ok(Self {
            id,
            socket,
            shutdown,
            drain_timeout,
        })
    }

    pub(crate) async fn run(mut self) {
        let mut buf = [0u8; MAX_UDP_SIZE];
        let mut queries: JoinSet<()> = JoinSet::new();

        loop {
            tokio::select! {
                result = self.socket.recv_from(&mut buf) => {
                    match result {
                        Ok((len, peer)) => {
                            let data = Bytes::copy_from_slice(&buf[..len]);
                            let socket = Arc::clone(&self.socket);
                            queries.spawn(async move {
                                if let Err(e) = handle_query(&socket, data, peer).await {
                                    warn!(peer = %peer, err = %e, "query error");
                                }
                            });
                        }
                        Err(e) => warn!(worker = self.id, err = %e, "recv error"),
                    }
                }
                // reap completed tasks so the set doesn't grow without bound
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
                // Drive the set to empty so task destructors run before we return.
                while queries.join_next().await.is_some() {}
            }
        }
    }
}

async fn handle_query(socket: &UdpSocket, data: Bytes, peer: SocketAddr) -> Result<(), QueryError> {
    let query = dnser_proto::Message::parse(data)?;
    debug!(id = query.header.id, peer = %peer, "query");
    let response = build_response(query);
    let bytes = response.to_bytes()?;
    socket.send_to(&bytes, peer).await?;
    Ok(())
}
