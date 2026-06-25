use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use dnser_cache::Cache;
use dnser_resolver::Resolver;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};
use tracing::{debug, warn};

use crate::error::QueryError;
use crate::handler::process_query;

pub(crate) struct TcpWorker {
    listener: TcpListener,
    resolver: Arc<Resolver>,
    cache: Arc<Cache>,
    shutdown: watch::Receiver<bool>,
    idle_timeout: Duration,
    connection_limit: Arc<Semaphore>,
}

impl TcpWorker {
    pub(crate) fn bind(
        addr: SocketAddr,
        resolver: Arc<Resolver>,
        cache: Arc<Cache>,
        shutdown: watch::Receiver<bool>,
        idle_timeout: Duration,
        max_connections: usize,
    ) -> Result<Self, std::io::Error> {
        let std_listener = std::net::TcpListener::bind(addr)?;
        std_listener.set_nonblocking(true)?;
        let listener = TcpListener::from_std(std_listener)?;
        Ok(Self {
            listener,
            resolver,
            cache,
            shutdown,
            idle_timeout,
            connection_limit: Arc::new(Semaphore::new(max_connections)),
        })
    }

    pub(crate) fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub(crate) async fn run(mut self) {
        loop {
            tokio::select! {
                result = self.listener.accept() => {
                    match result {
                        Ok((stream, peer)) => {
                            match Arc::clone(&self.connection_limit).try_acquire_owned() {
                                Ok(permit) => {
                                    let resolver = Arc::clone(&self.resolver);
                                    let cache = Arc::clone(&self.cache);
                                    let idle_timeout = self.idle_timeout;
                                    tokio::spawn(async move {
                                        if let Err(e) = handle_conn(stream, &resolver, &cache, idle_timeout, permit).await {
                                            warn!(peer = %peer, err = %e, "tcp connection error");
                                        }
                                    });
                                }
                                Err(_) => {
                                    warn!(peer = %peer, "tcp connection limit reached, dropping");
                                }
                            }
                        }
                        Err(e) => warn!(err = %e, "tcp accept error"),
                    }
                }
                _ = self.shutdown.changed() => break,
            }
        }
    }
}

async fn handle_conn(
    mut stream: TcpStream,
    resolver: &Resolver,
    cache: &Cache,
    idle_timeout: Duration,
    _permit: OwnedSemaphorePermit,
) -> Result<(), QueryError> {
    loop {
        // Read the 2-byte length prefix (RFC 1035 §4.2.2).
        let mut len_buf = [0u8; 2];
        match tokio::time::timeout(idle_timeout, stream.read_exact(&mut len_buf)).await {
            Err(_) => return Ok(()), // idle timeout — clean close
            Ok(Ok(_)) => {}
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Ok(Err(e)) => return Err(e.into()),
        }

        let msg_len = u16::from_be_bytes(len_buf) as usize;
        if msg_len == 0 {
            return Ok(());
        }

        // Read the message body.
        let mut buf = vec![0u8; msg_len];
        match tokio::time::timeout(idle_timeout, stream.read_exact(&mut buf)).await {
            Err(_) => return Ok(()),
            Ok(Ok(_)) => {}
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Ok(Err(e)) => return Err(e.into()),
        }

        let query = dnser_proto::Message::parse(Bytes::from(buf))?;
        debug!(id = query.header.id, "tcp query");

        let response = process_query(resolver, cache, &query).await;
        let response_bytes = response.to_bytes()?;

        // Write the 2-byte length prefix followed by the response.
        let len = response_bytes.len() as u16;
        stream.write_all(&len.to_be_bytes()).await?;
        stream.write_all(&response_bytes).await?;
    }
}
