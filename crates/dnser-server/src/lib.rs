mod error;
mod handler;
mod tcp;
mod worker;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use dnser_cache::Cache;
use dnser_config::Config;
use dnser_resolver::Resolver;
use tokio::sync::{Semaphore, watch};
use tokio::task::JoinHandle;
use tracing::info;

use tcp::TcpWorker;
use worker::Worker;

/// Handle to a running server. Call [`ServerHandle::shutdown`] to stop it.
pub struct ServerHandle {
    pub udp_addr: SocketAddr,
    pub tcp_addr: SocketAddr,
    shutdown_tx: watch::Sender<bool>,
    handles: Vec<JoinHandle<()>>,
    reaper: JoinHandle<()>,
}

impl ServerHandle {
    /// Signals all workers to stop, waits for them to finish draining, and
    /// aborts the background cache-reaper task.
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        self.reaper.abort();
        for h in self.handles {
            let _ = h.await;
        }
    }
}

/// Binds sockets and spawns all server tasks.
///
/// Returns immediately with a [`ServerHandle`]; the server is already
/// accepting queries. Use this in tests or embeddings that need to control
/// the server lifecycle programmatically. For a standalone binary, prefer
/// [`run`], which additionally waits for a shutdown signal.
pub async fn start(config: Config) -> Result<ServerHandle, std::io::Error> {
    let server_cfg = config.server;
    let num_workers = server_cfg.workers;
    let drain_timeout = Duration::from_secs(server_cfg.shutdown_drain_secs);
    let idle_timeout = Duration::from_secs(server_cfg.tcp_idle_timeout_secs);
    let max_connections = server_cfg.tcp_max_connections;
    let udp_inflight = Arc::new(Semaphore::new(server_cfg.udp_max_inflight));

    let resolver = Arc::new(Resolver::new(config.resolver).await?);
    let cache = Arc::new(Cache::new(
        config.cache.max_entries,
        config.cache.max_negative_ttl_secs as u32,
    ));
    let reaper_interval = Duration::from_secs(config.cache.reaper_interval_secs);

    let cache_reaper = Arc::clone(&cache);
    let reaper = tokio::spawn(async move {
        let mut interval = tokio::time::interval(reaper_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            cache_reaper.evict_expired();
        }
    });

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mut handles = Vec::with_capacity(num_workers + 1);

    // Bind the first worker to learn the actual UDP address (relevant when port = 0).
    let first_worker = Worker::bind(
        0,
        server_cfg.listen,
        Arc::clone(&resolver),
        Arc::clone(&cache),
        shutdown_rx.clone(),
        drain_timeout,
        Arc::clone(&udp_inflight),
    )?;
    let udp_addr = first_worker.local_addr()?;
    handles.push(tokio::spawn(first_worker.run()));

    for id in 1..num_workers {
        let w = Worker::bind(
            id,
            server_cfg.listen,
            Arc::clone(&resolver),
            Arc::clone(&cache),
            shutdown_rx.clone(),
            drain_timeout,
            Arc::clone(&udp_inflight),
        )?;
        handles.push(tokio::spawn(w.run()));
    }

    let tcp_worker = TcpWorker::bind(
        server_cfg.listen,
        Arc::clone(&resolver),
        Arc::clone(&cache),
        shutdown_rx,
        idle_timeout,
        max_connections,
    )?;
    let tcp_addr = tcp_worker.local_addr()?;
    handles.push(tokio::spawn(tcp_worker.run()));

    info!(udp = %udp_addr, tcp = %tcp_addr, workers = num_workers, "DNS server listening");

    Ok(ServerHandle {
        udp_addr,
        tcp_addr,
        shutdown_tx,
        handles,
        reaper,
    })
}

/// Runs the server until a shutdown signal (Ctrl-C / SIGTERM) is received.
pub async fn run(config: Config) -> Result<(), std::io::Error> {
    let handle = start(config).await?;
    shutdown_signal().await;
    info!("shutting down");
    handle.shutdown().await;
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = signal(SignalKind::terminate()).expect("failed to register SIGTERM");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for ctrl_c");
    }
}
