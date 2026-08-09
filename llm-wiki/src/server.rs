use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use rmcp::ServiceExt;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::config;
use crate::engine::WikiEngine;
use crate::mcp::McpServer;

// ── serve_stdio ───────────────────────────────────────────────────────────────

async fn serve_stdio(server: McpServer, mut shutdown: watch::Receiver<bool>) -> Result<()> {
    let transport = rmcp::transport::io::stdio();
    // serve() blocks on the MCP initialize handshake — with no client
    // speaking on stdin it would never resolve, so the shutdown signal
    // must be able to cancel the wait-for-client too.
    let service = tokio::select! {
        s = server.serve(transport) => {
            s.map_err(|e| anyhow::anyhow!("failed to start MCP stdio server: {e}"))?
        }
        _ = shutdown.changed() => {
            tracing::info!("stdio: shutdown before a client connected");
            return Ok(());
        }
    };

    tokio::select! {
        result = service.waiting() => {
            result.map_err(|e| anyhow::anyhow!("MCP stdio server error: {e}"))?;
        }
        _ = shutdown.changed() => {
            tracing::info!("stdio: shutdown signal received");
        }
    }
    Ok(())
}

// ── serve_http ────────────────────────────────────────────────────────────────

async fn serve_http(
    server: McpServer,
    port: u16,
    serve_cfg: &config::ServeConfig,
    any_host: bool,
    cancel: CancellationToken,
) -> Result<()> {
    let addr: SocketAddr = ([0, 0, 0, 0], port).into();

    // Host-header checking is DNS-rebinding protection for local serving.
    // An appliance dialed by its network address needs it off (--any-host):
    // the boundary there is the network, and address-enumeration in config
    // would go stale on every DHCP lease.
    let config =
        StreamableHttpServerConfig::default().with_cancellation_token(cancel.child_token());
    let config = if any_host {
        config.disable_allowed_hosts()
    } else {
        config.with_allowed_hosts(serve_cfg.http_allowed_hosts.clone())
    };

    let service: StreamableHttpService<McpServer, LocalSessionManager> =
        StreamableHttpService::new(move || Ok(server.clone()), Default::default(), config);

    let router = axum::Router::new().nest_service("/mcp", service);

    let max_attempts = if serve_cfg.max_restarts == 0 {
        1
    } else {
        serve_cfg.max_restarts
    };
    let mut backoff = std::time::Duration::from_secs(serve_cfg.restart_backoff as u64);
    let max_backoff = std::time::Duration::from_secs(30);

    for attempt in 1..=max_attempts {
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                tracing::info!(%addr, "HTTP server listening");
                axum::serve(listener, router)
                    .with_graceful_shutdown(cancel.cancelled_owned())
                    .await
                    .map_err(|e| anyhow::anyhow!("HTTP server error: {e}"))?;
                return Ok(());
            }
            Err(e) => {
                if attempt == max_attempts {
                    return Err(anyhow::anyhow!(
                        "HTTP bind failed after {max_attempts} attempts: {e}"
                    ));
                }
                tracing::warn!(
                    %addr,
                    error = %e,
                    attempt,
                    max = max_attempts,
                    "HTTP bind failed, retrying",
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(max_backoff);
            }
        }
    }
    unreachable!()
}

// ── serve (orchestration) ─────────────────────────────────────────────────────

/// Start the wiki server — spawns stdio, HTTP, ACP, and watcher transports as configured.
pub async fn serve(
    config_path: &std::path::Path,
    http_flag: Option<Option<u16>>,
    any_host: bool,
    acp: bool,
    watch: bool,
) -> Result<()> {
    // 1. Build WikiEngine
    let manager = Arc::new(WikiEngine::build(config_path)?);

    let (wiki_count, serve_cfg, http_enabled, resolved_port) = {
        let engine = manager.state.read().map_err(|_| anyhow::anyhow!("lock"))?;
        let count = engine.wikis.len();
        let cfg = engine.config.serve.clone();
        // Bare --http enables the transport on the config port; an
        // explicit port overrides it.
        let http = http_flag.is_some() || cfg.http;
        let port = http_flag.flatten().unwrap_or(cfg.http_port);
        (count, cfg, http, port)
    };

    // 2. Log startup summary — http and stdio are alternatives, not a pair
    let mut transports = if http_enabled {
        vec![format!("http :{resolved_port}")]
    } else {
        vec!["stdio".to_string()]
    };
    if acp {
        transports.push("acp".to_string());
    }
    if watch {
        transports.push("watch".to_string());
    }
    tracing::info!(
        wikis = wiki_count,
        transports = %transports.join("] ["),
        "server started",
    );

    // 3. Shutdown coordination
    let cancel = CancellationToken::new();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // ctrl_c handler
    let cancel_for_signal = cancel.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("shutdown signal received");
        cancel_for_signal.cancel();
        let _ = shutdown_tx.send(true);
    });

    // 4. Build MCP server
    let mcp_server = McpServer::new(manager.clone());

    // 5. Heartbeat task
    if serve_cfg.heartbeat_secs > 0 {
        let interval_secs = serve_cfg.heartbeat_secs;
        let cancel_hb = cancel.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(interval_secs as u64));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        tracing::debug!("heartbeat");
                    }
                    _ = cancel_hb.cancelled() => {
                        break;
                    }
                }
            }
        });
    }

    // 6. Start watcher (if enabled)
    // Push channel: watcher sends (wiki_name, message) to ACP sessions.
    // The original sender is dropped after spawning the watcher; only the watcher clone remains.
    let (push_tx, push_rx) = tokio::sync::mpsc::channel::<(String, String)>(64);

    let watch_handle = if watch {
        let watch_manager = manager.clone();
        let cancel_watch = cancel.clone();
        let debounce = {
            let engine = manager.state.read().map_err(|_| anyhow::anyhow!("lock"))?;
            engine.config.watch.debounce_ms
        };
        let push_tx_watch = push_tx;
        Some(tokio::spawn(async move {
            if let Err(e) =
                crate::watch::run_watcher(watch_manager, debounce, cancel_watch, push_tx_watch)
                    .await
            {
                tracing::error!(error = %e, "watcher error");
            }
        }))
    } else {
        drop(push_tx);
        None
    };

    // 7. Start transports
    if acp {
        let acp_manager = manager.clone();
        let cancel_acp = cancel.clone();
        let acp_sessions: crate::acp::Sessions = Arc::new(Mutex::new(HashMap::new()));
        let acp_serve_cfg = serve_cfg.clone();

        let acp_handle = tokio::spawn(async move {
            tokio::select! {
                result = crate::acp::serve_acp(acp_manager, acp_serve_cfg, acp_sessions, push_rx) => {
                    if let Err(e) = result {
                        tracing::error!(transport = "acp", error = %e, "ACP transport error");
                    }
                }
                _ = cancel_acp.cancelled() => {
                    tracing::info!("ACP: shutdown signal received");
                }
            }
        });

        if http_enabled {
            serve_http(mcp_server, resolved_port, &serve_cfg, any_host, cancel).await?;
        } else {
            serve_stdio(mcp_server, shutdown_rx).await?;
        }

        acp_handle.abort();
        let _ = acp_handle.await;
    } else if http_enabled {
        serve_http(mcp_server, resolved_port, &serve_cfg, any_host, cancel).await?;
    } else {
        serve_stdio(mcp_server, shutdown_rx).await?;
    }

    if let Some(handle) = watch_handle {
        handle.abort();
        let _ = handle.await;
    }

    tracing::info!("server stopped");
    Ok(())
}
