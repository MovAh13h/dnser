mod error;
mod handler;
mod worker;

use dnser_config::ServerConfig;
use tracing::info;
use worker::Worker;

pub async fn run(server_config: ServerConfig) -> Result<(), std::io::Error> {
    let workers = server_config.workers;

    let mut handles = Vec::with_capacity(workers);
    for id in 0..workers {
        let worker = Worker::bind(id, server_config.listen)?;
        handles.push(tokio::spawn(worker.run()));
    }

    info!(addr = %server_config.listen, workers = workers, "DNS server listening");

    shutdown_signal().await;
    info!("shutting down");

    for handle in &handles {
        handle.abort();
    }
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
