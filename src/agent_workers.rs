// ═════════════════════════════════════════════════════════════════════════════
// Agent Worker Extension Point
// Stub for external image generation (or other specialized) agents.
// ═════════════════════════════════════════════════════════════════════════════

/// Trait for external worker processes that can register as Avalon agents.
/// Workers run outside the main Avalon process and communicate over HTTP
/// or stdin/stdout. This isolates heavy models (image generation, etc.)
/// from the core async runtime.
#[async_trait::async_trait]
pub trait AgentWorker: Send + Sync {
    /// Unique worker name (must match an entry in the `agents` table).
    fn name(&self) -> &str;

    /// Human-readable description of what this worker does.
    fn description(&self) -> &str;

    /// Initialize the worker (e.g. spawn child process, connect to HTTP service).
    async fn start(&mut self) -> Result<(), String>;

    /// Gracefully shut down the worker.
    async fn stop(&mut self) -> Result<(), String>;

    /// Send a task payload and await a JSON result.
    /// The payload format is worker-specific.
    async fn dispatch(&self, task: serde_json::Value) -> Result<serde_json::Value, String>;
}

/// Registry of loaded agent workers.
pub struct WorkerRegistry {
    workers: Vec<Box<dyn AgentWorker>>,
}

impl WorkerRegistry {
    pub fn new() -> Self {
        Self { workers: Vec::new() }
    }

    pub fn register(&mut self, worker: Box<dyn AgentWorker>) {
        self.workers.push(worker);
    }

    pub fn get(&self, name: &str) -> Option<&dyn AgentWorker> {
        self.workers.iter().find(|w| w.name() == name).map(|w| w.as_ref())
    }

    pub fn list(&self) -> Vec<&dyn AgentWorker> {
        self.workers.iter().map(|w| w.as_ref()).collect()
    }

    pub async fn start_all(&mut self) -> Result<(), String> {
        for worker in &mut self.workers {
            worker.start().await?;
        }
        Ok(())
    }

    pub async fn stop_all(&mut self) -> Result<(), String> {
        for worker in &mut self.workers {
            worker.stop().await?;
        }
        Ok(())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Example placeholder worker implementations
// ═════════════════════════════════════════════════════════════════════════════

/// Placeholder for an HTTP-based image generation worker (e.g. Stable Diffusion API).
pub struct HttpImageWorker {
    name: String,
    endpoint: String,
}

impl HttpImageWorker {
    pub fn new(name: &str, endpoint: &str) -> Self {
        Self {
            name: name.to_string(),
            endpoint: endpoint.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl AgentWorker for HttpImageWorker {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "HTTP-based image generation worker (placeholder)."
    }

    async fn start(&mut self) -> Result<(), String> {
        // TODO: Verify endpoint is reachable
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), String> {
        Ok(())
    }

    async fn dispatch(&self, _task: serde_json::Value) -> Result<serde_json::Value, String> {
        Err("HttpImageWorker not yet implemented".to_string())
    }
}
