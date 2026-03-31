use std::net::SocketAddr;
use std::sync::Arc;
use axum::{Router, routing::post, routing::get};
use tokio::sync::{oneshot, RwLock};
use crate::provider::Provider;
use super::handlers::{handle_anthropic_message, handle_chat_completion, handle_responses, health_check};

const PROXY_PORT: u16 = 13721;

pub struct ProxyServer {
    provider: Arc<RwLock<Provider>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    // Keep a handle to the join handle so we can wait on it during shutdown
    server_task: Option<tokio::task::JoinHandle<()>>,
}

impl ProxyServer {
    pub fn new(provider: Provider) -> Self {
        Self {
            provider: Arc::new(RwLock::new(provider)),
            shutdown_tx: None,
            server_task: None,
        }
    }

    pub fn start(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        self.shutdown_tx = Some(shutdown_tx);

        let provider = self.provider.clone();

        // Spawn the server in a separate task instead of blocking
        let task = tokio::spawn(async move {
            let app = Router::new()
                .route("/v1/chat/completions", post(handle_chat_completion))
                .route("/v1/responses", post(handle_responses))
                .route("/v1/messages", post(handle_anthropic_message))
                .route("/health", get(health_check))
                .with_state(provider);

            let addr = SocketAddr::from(([127, 0, 0, 1], PROXY_PORT));
            tracing::info!("Starting proxy server on {}", addr);

            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!("Failed to bind to {}: {}", addr, e);
                    return;
                }
            };

            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await;

            tracing::info!("Proxy server stopped");
        });

        self.server_task = Some(task);
        tracing::info!("Proxy server spawned");
        Ok(())
    }

    /// Switch to a different provider without restarting the server
    pub async fn switch_provider(&self, provider: Provider) {
        let mut p = self.provider.write().await;
        *p = provider;
        tracing::info!("Provider switched dynamically");
    }

    /// Get the current provider (for reading in handlers)
    pub fn get_provider(&self) -> Arc<RwLock<Provider>> {
        self.provider.clone()
    }

    pub async fn stop(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        // Wait for the server task to finish
        if let Some(task) = self.server_task.take() {
            let _ = task.await;
        }

        Ok(())
    }
}
