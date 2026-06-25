mod error;
mod handler;
mod worker;

use std::sync::Arc;
use std::time::Duration;

use dnser_cache::Cache;
use dnser_config::Config;
use dnser_resolver::Resolver;
use tokio::sync::watch;
use tracing::info;
use worker::Worker;

pub async fn run(config: Config) -> Result<(), std::io::Error> {
    let server_config = config.server;
    let workers = server_config.workers;
    let drain_timeout = Duration::from_secs(server_config.shutdown_drain_secs);

    let resolver = Arc::new(Resolver::new(config.resolver).await?);
    let cache = Arc::new(Cache::new(config.cache.max_entries));
    let reaper_interval = Duration::from_secs(config.cache.reaper_interval_secs);

    let cache_reaper = Arc::clone(&cache);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(reaper_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            cache_reaper.evict_expired();
        }
    });

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let mut handles = Vec::with_capacity(workers);
    for id in 0..workers {
        let worker = Worker::bind(
            id,
            server_config.listen,
            Arc::clone(&resolver),
            Arc::clone(&cache),
            shutdown_rx.clone(),
            drain_timeout,
        )?;
        handles.push(tokio::spawn(worker.run()));
    }

    info!(addr = %server_config.listen, workers, "DNS server listening");

    shutdown_signal().await;
    info!("shutting down");

    let _ = shutdown_tx.send(true);

    for handle in handles {
        let _ = handle.await;
    }

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
