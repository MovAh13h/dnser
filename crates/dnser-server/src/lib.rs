mod error;
mod handler;
mod worker;

use tracing::info;
use worker::Worker;

const PORT: u16 = 1053;

pub async fn run() -> Result<(), std::io::Error> {
    let cpus = std::thread::available_parallelism()?.get();

    let mut handles = Vec::with_capacity(cpus);
    for id in 0..cpus {
        let worker = Worker::bind(id, PORT)?;
        handles.push(tokio::spawn(worker.run()));
    }

    info!(port = PORT, workers = cpus, "DNS server listening");

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
