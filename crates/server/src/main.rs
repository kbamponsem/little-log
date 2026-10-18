//! Little Log server entry point.
//!
//! Environment:
//!   PORT            listen port (default 8080)
//!   DATA_DIR        SQLite file + photos (default /data)
//!   WEB_DIR         built frontend to serve (default ./web/dist)
//!   SESSION_SECRET  HMAC key for sessions and invites (≥16 chars)
//!   COOKIE_SECURE   "1" to always mark cookies Secure (behind HTTPS)

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use little_log_server::auth::new_token;
use little_log_server::build_router;
use little_log_server::state::{AppState, Config};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // `little-log-server healthcheck` — what Docker's HEALTHCHECK runs. A raw
    // TCP request keeps the runtime image free of curl/wget.
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        return healthcheck();
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,tower_http=warn")),
        )
        .init();

    let config = Config {
        session_secret: session_secret()?,
        cookie_secure: flag("COOKIE_SECURE"),
        data_dir: PathBuf::from(env_or("DATA_DIR", "/data")),
        web_dir: PathBuf::from(env_or("WEB_DIR", "./web/dist")),
    };
    let port: u16 = env_or("PORT", "8080").parse()?;

    let serving_ui = config.web_dir.join("index.html").is_file();
    tracing::info!(
        "Little Log: database {}, photos {}, {}",
        config.database().display(),
        config.photos().display(),
        if serving_ui {
            format!("serving the UI from {}", config.web_dir.display())
        } else {
            format!("no UI at {} — API only", config.web_dir.display())
        }
    );

    let state = AppState::new(config)?;
    let router = build_router(state);

    let address = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
    let listener = TcpListener::bind(address).await?;
    tracing::info!("listening on http://{address}");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Ask `/healthz` over TCP; exits non-zero when the server is not answering.
fn healthcheck() -> Result<(), Box<dyn std::error::Error>> {
    let port: u16 = env_or("PORT", "8080").parse()?;
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));

    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(3))?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    stream.write_all(b"GET /healthz HTTP/1.0\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    if response.starts_with("HTTP/1.0 200") || response.starts_with("HTTP/1.1 200") {
        Ok(())
    } else {
        Err(format!("healthcheck got: {}", response.lines().next().unwrap_or("")).into())
    }
}

/// The HMAC key for sessions and invites. In production this must be stable:
/// changing it signs everyone out.
fn session_secret() -> Result<String, Box<dyn std::error::Error>> {
    match std::env::var("SESSION_SECRET") {
        Ok(secret) if secret.trim().len() >= 16 => {
            if secret.contains("change-me") || secret.contains("changeme") {
                tracing::warn!(
                    "SESSION_SECRET still looks like a placeholder — set a real one (`openssl rand -hex 32`)."
                );
            }
            Ok(secret)
        }
        Ok(_) => Err("SESSION_SECRET must be at least 16 characters".into()),
        Err(_) => {
            tracing::warn!(
                "SESSION_SECRET is not set — using a random one. Everyone will be signed out when this process restarts."
            );
            Ok(new_token())
        }
    }
}

fn env_or(name: &str, fallback: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| fallback.to_string())
}

fn flag(name: &str) -> bool {
    matches!(
        std::env::var(name).as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

/// Ctrl-C (or Docker's SIGTERM) stops accepting new requests.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    tracing::info!("shutting down");
}
