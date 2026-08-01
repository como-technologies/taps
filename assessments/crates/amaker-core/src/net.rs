//! Small networking / process helpers shared across the binaries.

use std::net::{IpAddr, UdpSocket};

/// Best-effort LAN-IP detection. Opens a UDP socket "toward" a public
/// address (no packet is actually sent — UDP `connect` only sets the
/// peer) and reads back the local-side IP the kernel would route from.
/// Returns `None` if the trick fails for any reason (no network, etc.).
pub fn detect_lan_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("1.1.1.1:80").ok()?;
    Some(socket.local_addr().ok()?.ip())
}

/// Resolves when the process receives SIGTERM or Ctrl-C (SIGINT).
///
/// Pass to `axum::serve(...).with_graceful_shutdown(shutdown_signal())`
/// so in-flight requests drain before the process exits. Cloud Run and
/// most container orchestrators send SIGTERM before killing a container;
/// without this, active requests are severed mid-flight.
pub async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received; draining in-flight requests");
}
