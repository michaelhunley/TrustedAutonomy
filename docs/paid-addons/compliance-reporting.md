# Compliance Reporting (Paid Add-On)

> **Layer**: L2 — Supervision & Policy
> **Status**: Planned
> **Boundary**: TA core provides the hash-chained audit log and decision reasoning. Compliance Reporting packages this into framework-specific evidence documents.

---

## What It Does

Transforms TA's audit trail, policy decisions, drift reports, and review records into compliance evidence packages aligned with specific regulatory frameworks.

## Reports

### ISO/IEC 42001 Evidence Package
- **AI Management System documentation**: Policy YAML → controls mapping
- **Human oversight records**: Draft review decisions with timestamps and reasoning
- **Capability boundary documentation**: Agent alignment profiles → granted/denied capabilities
- **Audit trail integrity**: Hash chain verification report
- **Behavioral monitoring**: Drift detection summaries per agent

### EU AI Act Compliance Report
- **Article 9 (Risk Management)**: Risk scoring per draft, escalation trigger documentation
- **Article 14 (Human Oversight)**: Review decision log, approval/denial rates, escalation response times
- **Article 50 (Transparency)**: Tool call interception records, action classification documentation
- **Provider obligations**: Agent identity declarations, capability manifests

### NIST AI RMF Mapping
- **Govern**: Policy document inventory, role-based access documentation
- **Map**: Resource mediation scope, URI scheme coverage
- **Measure**: Drift metrics, confidence scores, budget tracking
- **Manage**: Escalation history, remediation records

### Singapore IMDA Agentic AI Framework
- **Agent boundary documentation**: Per-agent capability manifests
- **Network governance**: Multi-agent alignment verification records
- **Coordination alignment**: Session event logs showing inter-agent mediation

## Integration with TA Core

Reads from TA's existing data stores:
- `.ta/audit.jsonl` — hash-chained event log
- `.ta/goals/` — goal lifecycle records
- `.ta/pr_packages/` — draft review decisions
- `.ta/policy.yaml` + `agents/*.yaml` + `.ta/constitutions/` — policy configuration

Outputs:
- PDF/HTML evidence packages
- Structured JSON for ingestion by GRC platforms
- Exportable compliance dashboards

## Delivery Model

- Standalone CLI tool: `ta-compliance generate --framework iso42001 --period 2024-Q4`
- Web dashboard for ongoing monitoring
- No runtime dependency on TA core — reads from `.ta/` data stores
