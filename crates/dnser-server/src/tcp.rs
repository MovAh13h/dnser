use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use dnser_cache::Cache;
use dnser_net::{read_framed, write_framed};
use dnser_resolver::Resolver;
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
        connection_limit: Arc<Semaphore>,
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
            connection_limit,
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
        let body = match tokio::time::timeout(idle_timeout, read_framed(&mut stream)).await {
            Err(_) => return Ok(()), // idle timeout — clean close
            Ok(Ok(Some(body))) => body,
            Ok(Ok(None)) => return Ok(()),
            Ok(Err(e)) => return Err(e.into()),
        };

        let query = dnser_proto::Message::parse(body)?;
        debug!(id = query.header.id, "tcp query");

        let response = process_query(resolver, cache, &query).await;
        let response_bytes = response.to_bytes()?;
        write_framed(&mut stream, &response_bytes).await?;
    }
}
