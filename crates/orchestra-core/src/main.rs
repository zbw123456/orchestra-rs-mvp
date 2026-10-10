mod api;
mod domain;
mod engine;
mod runtime;
mod scheduler;
mod store;

use anyhow::{Context, Result};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
};
use tokio_util::sync::CancellationToken;

fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.into())
}

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--demo-worker") {
        return runtime::demo_worker();
    }
    let database = env("ORCHESTRA_DB", "data/orchestra.db");
    if let Some(parent) = Path::new(&database)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    // Bind before opening the DB so accidentally launching twice on the same port is harmless.
    let port: u16 = env("ORCHESTRA_PORT", "8080").parse()?;
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
            .await?;
    // A database is owned by exactly one core instance (including across different ports).
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(format!("{database}.lock"))?;
    lock.try_lock()
        .context("database already owned by another core")?;
    let providers = runtime::providers(std::env::var("ORCHESTRA_PROVIDERS").ok().as_deref())?;
    let scheduler =
        scheduler::Scheduler::new(env("ORCHESTRA_SCHEDULER_URL", "http://127.0.0.1:8091"))?;
    let stop = CancellationToken::new();
    let (engine, client) = engine::Engine::new(
        store::Store::open(&database)?,
        providers,
        scheduler,
        env("ORCHESTRA_STRATEGY", "aging"),
        env("ORCHESTRA_WORKERS", "2").parse()?,
        stop.clone(),
    )?;
    let engine_handle = tokio::spawn(engine.run());
    let signal = stop.clone();
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            let mut term =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("SIGTERM handler");
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
        signal.cancel();
    });
    println!("Orchestra experimental core listening on http://127.0.0.1:{port}");
    let result = axum::serve(listener, api::router(client))
        .with_graceful_shutdown(stop.clone().cancelled_owned())
        .await;
    stop.cancel();
    engine_handle.await??;
    result?;
    drop(lock);
    Ok(())
}
