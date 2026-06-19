mod error;
mod handler;
mod worker;

use std::time::Duration;

use dnser_config::ServerConfig;
use tokio::sync::watch;
use tracing::info;
use worker::Worker;

pub async fn run(server_config: ServerConfig) -> Result<(), std::io::Error> {
    let workers = server_config.workers;
    let drain_timeout = Duration::from_secs(server_config.shutdown_drain_secs);

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let mut handles = Vec::with_capacity(workers);
    for id in 0..workers {
        let worker = Worker::bind(id, server_config.listen, shutdown_rx.clone(), drain_timeout)?;
        handles.push(tokio::spawn(worker.run()));
    }

    info!(addr = %server_config.listen, workers, "DNS server listening");

    shutdown_signal().await;
    info!("shutting down");

    // tell all workers to stop accepting; they drain in-flight queries internally
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
