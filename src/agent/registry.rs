use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use once_cell::sync::Lazy;
use tokio::sync::Mutex;

use crate::error::{AgentError, AgentResult};

#[async_trait]
pub trait HandoffReceiver: Send + Sync {
    async fn receive_handoff(&self, context: serde_json::Value) -> AgentResult<()>;
}

#[derive(Default)]
pub struct AgentRegistry {
    agents: Mutex<HashMap<String, Arc<dyn HandoffReceiver>>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: Mutex::new(HashMap::new()),
        }
    }

    pub async fn register(&self, name: impl Into<String>, receiver: Arc<dyn HandoffReceiver>) {
        let mut agents = self.agents.lock().await;
        agents.insert(name.into(), receiver);
    }

    pub async fn unregister(&self, name: &str) {
        let mut agents = self.agents.lock().await;
        agents.remove(name);
    }

    pub async fn get(&self, name: &str) -> Option<Arc<dyn HandoffReceiver>> {
        let agents = self.agents.lock().await;
        agents.get(name).cloned()
    }

    pub async fn notify(&self, name: &str, context: serde_json::Value) -> AgentResult<()> {
        if let Some(receiver) = self.get(name).await {
            receiver.receive_handoff(context).await
        } else {
            Err(AgentError::InvalidConfig(format!(
                "handoff target not registered: {name}"
            )))
        }
    }

    pub async fn list(&self) -> Vec<String> {
        let agents = self.agents.lock().await;
        agents.keys().cloned().collect()
    }
}

static GLOBAL_REGISTRY: Lazy<AgentRegistry> = Lazy::new(AgentRegistry::new);

pub fn global_agent_registry() -> &'static AgentRegistry {
    &GLOBAL_REGISTRY
}
