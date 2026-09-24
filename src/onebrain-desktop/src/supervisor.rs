//! Desktop owns process resources; all product semantics stay node-owned.
use onebrain_node::OneBrainNode;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::{
    sync::{watch, Mutex},
    task::JoinHandle,
};

pub type SharedNode = Arc<Mutex<OneBrainNode>>;

/// Preserve the node-owned network shutdown order, then close Base and wait
/// for admitted KU work before the listener is released.
pub async fn stop_owned_node(node: &SharedNode) -> Result<(), &'static str> {
    let base = node.lock().await.base_services();
    node.lock().await.shutdown_network().await;
    if let Some(base) = base {
        base.close()
            .await
            .map_err(|_| "desktop_base_drain_failed")?;
    }
    Ok(())
}

/// In-process host provisioning only. Construct exactly one node with the
/// existing custody/dependency ports; no credentials can be supplied by Web IPC.
pub struct HostNode {
    pub node: OneBrainNode,
    /// Static diagnostic only; never includes operator paths, keys or source text.
    pub ku_issue: Option<&'static str>,
    #[cfg(feature = "vnext-outbound-first")]
    pub binding: Option<onebrain_api::obp_api::Binding>,
}

impl HostNode {
    pub fn local(node: OneBrainNode) -> Self {
        Self {
            node,
            ku_issue: None,
            #[cfg(feature = "vnext-outbound-first")]
            binding: None,
        }
    }
}

struct Resources {
    node: Option<SharedNode>,
    api: Option<JoinHandle<()>>,
    auxiliary: Option<JoinHandle<()>>,
}

pub struct Supervisor {
    stopping: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
    stop_accept: tokio_util::sync::CancellationToken,
    stop_sockets: tokio_util::sync::CancellationToken,
    execution: watch::Sender<bool>,
    resources: Mutex<Resources>,
}

impl Default for Supervisor {
    fn default() -> Self {
        let (execution, _) = watch::channel(false);
        Self {
            stopping: Arc::new(AtomicBool::new(false)),
            ready: Arc::new(AtomicBool::new(false)),
            execution,
            stop_accept: tokio_util::sync::CancellationToken::new(),
            stop_sockets: tokio_util::sync::CancellationToken::new(),
            resources: Mutex::new(Resources {
                node: None,
                api: None,
                auxiliary: None,
            }),
        }
    }
}

impl Supervisor {
    /// The trusted host must explicitly grant execution before constructing ports.
    /// Lifecycle withdrawal is permanent for this process.
    pub fn execution(&self) -> watch::Receiver<bool> {
        self.execution.subscribe()
    }
    pub fn grant_execution(&self) -> Result<(), &'static str> {
        if self.stopping.load(Ordering::SeqCst) {
            return Err("desktop_stopping");
        }
        self.execution.send_replace(true);
        // Close the race against an OS callback withdrawing execution.
        if self.stopping.load(Ordering::SeqCst) {
            self.execution.send_replace(false);
            return Err("desktop_stopping");
        }
        Ok(())
    }
    pub fn fence(&self) -> bool {
        let first = !self.stopping.swap(true, Ordering::SeqCst);
        self.ready.store(false, Ordering::SeqCst);
        self.execution.send_replace(false);
        self.stop_accept.cancel();
        first
    }
    pub fn ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst) && !self.stopping.load(Ordering::SeqCst)
    }
    pub fn stopped(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }

    /// Bind before publishing credentials. No port probing/fallback and no second node.
    pub async fn start(
        &self,
        host: HostNode,
        token: String,
        port: u16,
    ) -> Result<(SharedNode, u16), &'static str> {
        let mut resources = self.resources.lock().await;
        let node = Arc::new(Mutex::new(host.node));
        if self.stopped() || resources.node.is_some() {
            let _ = stop_owned_node(&node).await;
            return Err("desktop_already_started_or_stopping");
        }
        let listener =
            match tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await {
                Ok(listener) => listener,
                Err(_) => {
                    let _ = stop_owned_node(&node).await;
                    return Err("desktop_api_bind_failed");
                }
            };
        let port = listener
            .local_addr()
            .map_err(|_| "desktop_api_address_failed")?
            .port();
        let server = onebrain_api::ApiServer::with_shared_node(node.clone(), token, port);
        #[cfg(feature = "vnext-outbound-first")]
        let server = match host.binding {
            Some(binding) => server.with_obp_binding(binding),
            None => server,
        };
        // The shared router retains its authentication and route inventory.
        // This outer layer admits only the two packaged Tauri origins.
        let mut origins = vec![
            "tauri://localhost"
                .parse::<axum::http::HeaderValue>()
                .unwrap(),
            "http://tauri.localhost".parse().unwrap(),
        ];
        if cfg!(debug_assertions) {
            origins.push("http://localhost:5173".parse().unwrap());
        }
        let cors = tower_http::cors::CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::PUT,
                axum::http::Method::PATCH,
                axum::http::Method::DELETE,
                axum::http::Method::OPTIONS,
            ])
            .allow_headers([
                axum::http::header::AUTHORIZATION,
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderName::from_static("x-onebrain-obp-management"),
                axum::http::HeaderName::from_static("x-onebrain-vnext-client-session"),
            ]);
        let stopping = self.stopping.clone();
        let router = server
            .build_router()
            .layer(cors)
            .layer(axum::middleware::from_fn(
                move |request: axum::extract::Request, next: axum::middleware::Next| {
                    let stopping = stopping.clone();
                    async move {
                        if stopping.load(Ordering::SeqCst) {
                            use axum::response::IntoResponse;
                            axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response()
                        } else {
                            next.run(request).await
                        }
                    }
                },
            ));
        if self.stopped() {
            let _ = stop_owned_node(&node).await;
            return Err("desktop_stopping");
        }
        resources.node = Some(node.clone());
        let listener = crate::local_listener::LocalListener {
            listener,
            cancel: self.stop_sockets.clone(),
        };
        let stop = self.stop_accept.clone();
        let ready = self.ready.clone();
        resources.api = Some(tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(stop.cancelled_owned())
                .await;
            ready.store(false, Ordering::SeqCst);
        }));
        self.ready.store(true, Ordering::SeqCst);
        Ok((node, port))
    }

    pub async fn own_auxiliary(&self, task: JoinHandle<()>) {
        let mut resources = self.resources.lock().await;
        if self.stopped() || resources.auxiliary.is_some() {
            task.abort();
            let _ = task.await;
        } else {
            resources.auxiliary = Some(task);
        }
    }

    pub async fn shutdown(&self) -> Result<(), &'static str> {
        self.fence();
        let mut resources = self.resources.lock().await;
        // Stop accepting requests first. Network owners shut down before the
        // Base gate drains admitted KU work and upgraded sockets are canceled.
        if let Some(task) = resources.auxiliary.take() {
            task.abort();
            let _ = task.await;
        }
        if let Some(node) = resources.node.as_ref() {
            stop_owned_node(node).await?;
        }
        resources.node.take();
        self.stop_sockets.cancel();
        if let Some(task) = resources.api.take() {
            let _ = task.await;
        }
        Ok(())
    }
}

/// Same canonical status read as the API, reduced to finite, non-sensitive text.
/// It is a snapshot taken on an explicit tray action, never global availability.
pub async fn tray_status(node: Option<SharedNode>) -> &'static str {
    #[cfg(feature = "vnext-outbound-first")]
    if let Some(node) = node {
        let services = node.lock().await.vnext_product_services();
        if let Some(services) = services {
            if let Ok(status) = services.obp_status().await {
                return match (status["lifecycle"].as_str(), status["coverage"].as_str()) {
                    (Some("disabled"), _) => "OBP disabled - local observation",
                    (Some("active"), Some("partial")) => "OBP active - partial observation",
                    (Some("requested"), _) => "OBP requested - partial observation",
                    (Some("degraded"), _) => "OBP degraded - partial observation",
                    _ => "OBP unavailable - local observation",
                };
            }
        }
    }
    #[cfg(not(feature = "vnext-outbound-first"))]
    let _ = node;
    "OBP unavailable - local observation"
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.fence();
        self.stop_sockets.cancel();
        let resources = self.resources.get_mut();
        if let Some(task) = resources.api.take() {
            task.abort();
        }
        if let Some(task) = resources.auxiliary.take() {
            task.abort();
        }
    }
}
