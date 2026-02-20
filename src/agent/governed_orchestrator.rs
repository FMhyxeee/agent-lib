use crate::agent::{
    GovernanceInjectionRequest, GovernanceInjectionResult, GovernedOrchestrationResult,
    OrchestrationRequest, Orchestrator,
};
use crate::error::AgentResult;

/// Governance-aware adapter around the base orchestrator.
///
/// This adapter is non-breaking: if governance input is missing, execution
/// behaves the same as `Orchestrator::execute`.
pub struct GovernedOrchestrator {
    inner: Orchestrator,
}

impl GovernedOrchestrator {
    pub fn new(inner: Orchestrator) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &Orchestrator {
        &self.inner
    }

    pub fn into_inner(self) -> Orchestrator {
        self.inner
    }

    pub async fn execute(
        &self,
        request: OrchestrationRequest,
        governance: Option<GovernanceInjectionRequest>,
    ) -> AgentResult<GovernedOrchestrationResult> {
        if let Some(governance) = governance {
            let (orchestration, governance) = self
                .inner
                .execute_with_governance(request, governance)
                .await?;
            Ok(GovernedOrchestrationResult {
                orchestration,
                governance,
            })
        } else {
            let orchestration = self.inner.execute(request).await?;
            Ok(GovernedOrchestrationResult {
                orchestration,
                governance: GovernanceInjectionResult {
                    preflight_applied: false,
                    preflight_context: None,
                    postrun_applied: false,
                    postrun_context: None,
                },
            })
        }
    }
}
