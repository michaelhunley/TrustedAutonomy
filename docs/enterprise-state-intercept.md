# Enterprise State Intercept — Deep Agent Containment

> Extracted from PLAN.md v0.7. This capability addresses enterprise environments where agents may bypass MCP and make direct HTTP/network calls, or where regulatory requirements demand network-level proof of containment. For most users, MCP interception (v0.5) is sufficient.

## The Problem

MCP interception (v0.5) captures agent actions that go through MCP tool calls. But agents can also:
- Make direct HTTP requests (curl, reqwest, fetch) bypassing MCP entirely
- Execute shell commands that have network side effects (git push, ssh, scp)
- Use SDKs that talk to APIs directly (boto3, google-cloud-python)
- Run code that opens sockets, sends UDP, or uses protocols MCP doesn't cover

For **enterprise and regulated environments**, this gap matters. The original TA vision addressed this with a VM/embedded shell model — all agent I/O passes through a controlled boundary. This document captures that vision for future implementation.

## Original VM Model (from WHY-TA-vs-VM.md)

The VM approach provides kernel-level isolation: agent runs in a contained environment, all network traffic routed through a controlled proxy, filesystem changes captured via snapshots. TA's staging approach is lighter but doesn't capture network-level state changes.

**The hybrid model**: TA's staging-based governance (lightweight, semantic, human-reviewable) combined with network-level capture (comprehensive, binary, automated) gives defense in depth:
- **Layer 1 (TA staging)**: File changes captured, diffed, reviewed. Works today.
- **Layer 2 (MCP interception, v0.5)**: MCP tool calls captured, reviewed. Covers well-behaved agents.
- **Layer 3 (Network intercept)**: All outbound state-changing traffic captured, regardless of how the agent initiated it. Covers everything.

## Architecture

### Network Traffic Capture & Governance

**Core**: Transparent proxy that captures network traffic from agent processes, classifies it, and holds state-changing requests at a checkpoint.

- **Capture layer**: Transparent proxy (mitmproxy-based or custom Rust) that agent traffic routes through. Requires sandbox (v0.6) or network namespace to force routing.
- **Traffic classification**:
  - Read-only (GET, search queries) -> pass through, log for audit
  - State-changing (POST, PUT, DELETE, form submissions) -> capture and hold in draft
  - Sensitive (auth tokens, PII, credentials) -> flag for review, never auto-approve
- **AI summary**: Each captured request gets an LLM-generated plain-English description
- **Draft integration**: Captured network actions appear in `ta draft view` alongside MCP actions and file changes. URI scheme: `net://api.gmail.com/POST/send`
- **Replay on apply**: Approved network requests replayed with auth refresh, idempotency keys, retry logic

### Research: Existing Tools (survey before building)

| Tool | Approach | Integration potential |
|------|----------|---------------------|
| mitmproxy / mitmproxy-rs | Transparent HTTPS proxy, Python/Rust API | Best candidate for capture layer |
| Envoy/Istio sidecar | Service mesh traffic interception | Over-engineered for single-agent use |
| eBPF (bpftrace, Cilium) | Kernel-level packet observation | No proxy needed but complex setup |
| Burp Suite / ZAP | Security-focused HTTP intercept | Plugin ecosystem, not embeddable |
| OpenTelemetry | Distributed tracing | Observation model, not interception |
| pcap/npcap | Raw packet capture | Too low-level, need structured data |

**Evaluation criteria**: transparent proxy for sandboxed process, TLS interception with local CA, structured data output (not raw bytes), Rust/C FFI, Apache-2.0/MIT license.

### LLM Traffic Intelligence (future)

- Protocol understanding: models trained on REST/GraphQL/gRPC patterns for accurate summaries
- Security intelligence: CVE/NVD integration, known-bad endpoints, credential leak patterns
- Anomaly detection: agent calling unfamiliar APIs, data exfiltration patterns
- Training pipeline: each approved/rejected traffic decision feeds back into the classifier

### Standalone Packaging (decision point)

Package as:
- (a) Built-in TA module
- (b) Standalone protocol/library that TA depends on
- (c) Both — library with TA as reference implementation

Criteria: community interest, standalone utility, maintenance burden.

## When to Build This

**Prerequisites**: MCP interception (v0.5) must ship first. If MCP coverage proves sufficient for 80%+ of use cases, this becomes an enterprise add-on rather than a core requirement.

**Triggers for implementation**:
- Enterprise customers require network-level proof of containment
- Agents routinely bypass MCP for direct API calls
- Regulatory audits demand network-level evidence (not just MCP traces)
- Security incidents where MCP interception missed a state change

**Estimated scope**: 3-4 phases (research, capture, intelligence, packaging). Depends on v0.6 sandbox for traffic routing.

## Relationship to Community Memory

Community memory (shared knowledge base of solved problems) was originally placed after network intelligence. Without the enterprise intercept layer, community memory is still valuable but limited to code-level patterns and MCP action patterns. It can ship earlier as a standalone feature, with network traffic signatures added later if/when this layer ships.

## Standards Alignment

- **Singapore IMDA Agentic AI Framework**: Agent boundary enforcement at network level
- **NIST AI RMF GOVERN 1.4**: Containment processes for AI risk management
- **EU AI Act Article 9**: Risk management including network-level monitoring for high-risk systems
- **ISO/IEC 42001**: Network-level provenance for complete audit trails
