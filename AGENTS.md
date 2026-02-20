# AGENTS.md

Primary contributor guide: `CLAUDE.md`.

Repository-specific note:
- `agent-lib` now uses a flattened build; do not rely on crate feature flags for provider/tool/MCP/skills selection.
- Multi-agent governance is modeled as an outer adapter (`GovernedOrchestrator`), not a role inside `planner.parallel_targets`.
- Keep `Orchestrator::execute` behavior backward-compatible; governance-aware flow should use `GovernedOrchestrator` or `execute_with_governance`.
- Governance integration is soft-gating by default: issues enrich planner/reviewer context but do not block execution.
