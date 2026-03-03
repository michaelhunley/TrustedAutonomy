# Policy Studio (Paid Add-On)

> **Layer**: L2 — Supervision & Policy
> **Status**: Planned
> **Boundary**: TA's open-source policy engine evaluates any YAML. The Studio generates better YAML faster.

---

## What It Does

An interactive tool that helps users generate, validate, and refine `.ta/policy.yaml` and related policy files. It does not replace the open-source engine — it produces YAML that the engine consumes.

## Features

### Policy Generation
- **Conversational wizard**: "What does your agent need to do?" → produces a complete `.ta/policy.yaml` with appropriate scheme rules, escalation triggers, and security level.
- **Templates**: Pre-built policy profiles for common use cases:
  - "Standard developer agent" — fs read/write, approval on apply/delete
  - "Read-only auditor" — fs read only, no write grants
  - "External communications agent" — email + social with approval on send
  - "Infrastructure operator" — cloud + db with approval on everything
- **Workflow-specific policies**: Generate `.ta/workflows/*.yaml` that layer on top of the project policy.

### Policy Validation
- **Conflict detection**: Identify contradictions between policy layers (project → workflow → agent → goal).
- **Coverage analysis**: "These agent capabilities have no policy rules — they'll be denied by default."
- **Escalation gap analysis**: "These actions are allowed but have no escalation triggers — consider adding monitoring."

### Compliance Mapping
- **ISO/IEC 42001**: Map TA policy rules to AI management system controls. Generate evidence of human oversight, capability boundaries, and audit trail integrity.
- **EU AI Act**: Map to Articles 9 (risk management), 14 (human oversight), 50 (transparency obligations).
- **Singapore IMDA Agentic AI Framework**: Map to agent boundary, network governance, and coordination alignment requirements.
- **NIST AI RMF**: Map to risk-proportional review and behavioral monitoring categories.

### Drift Analysis
- **Historical analysis**: "Based on the last 30 days of audit data, here's what your agents actually did vs. what policy allows."
- **Tightening recommendations**: "Agent X never used db:// access — consider removing it."
- **Anomaly detection**: "Agent Y's rejection rate increased 3x this week — review the escalation triggers."

## Integration with TA Core

The Studio produces standard TA YAML files:
- `.ta/policy.yaml` (project policy)
- `.ta/workflows/*.yaml` (workflow policies)
- `agents/*.yaml` (alignment profiles)
- `.ta/constitutions/*.yaml` (goal constitutions)

All output is reviewed via TA's own draft model — the Studio's proposals are artifacts in a TA draft that the human approves.

## Delivery Model

- Standalone CLI tool or web service
- Reads TA audit logs and policy files from `.ta/`
- Outputs YAML files the user reviews and commits
- No runtime dependency — TA core never calls the Studio at runtime
