# Agent Messaging Protocol (AMP)

**Status**: Draft — v0.1
**Owner**: Trusted Autonomy
**License**: Apache 2.0 (OSS standard)

---

## Overview

Agent Messaging Protocol (AMP) is a structured, embedding-native communication protocol for direct agent-to-agent interaction. It replaces natural-language intermediation between agents with typed message envelopes carrying semantic vector embeddings, structured payloads, and cryptographic audit trails.

**Core goals:**

| Goal | How AMP achieves it |
|---|---|
| Reduce token usage | Embeddings encode semantic content in 768–1536 floats; no prose round-trips |
| Increase clarity | Typed payloads eliminate ambiguity; schema-validated at send and receive |
| Enable full auditability | Every message logged with sender, receiver, embedding, timestamp, hash |
| Agent-agnostic | Works across Claude, Codex, local models, BMAD roles, custom agents |
| OSS composability | Protocol spec is standalone; TA ships the reference broker as a plugin |

---

## Problem Statement

Today, multi-agent workflows communicate through natural language:

```
Agent A writes: "Please implement the authentication module following the
                architecture we discussed, ensuring it uses JWT tokens and
                integrates with the existing user service..."

Agent B reads, tokenizes, embeds, plans, then writes back:
                "I have implemented the authentication module. It uses JWT
                tokens and integrates with the UserService via..."
```

This is expensive, lossy, and unauditable:
- The same semantic intent is re-encoded in prose at every hop
- Context that was already embedded gets serialized to text and re-embedded by the receiver
- There is no machine-readable audit record of what was actually communicated
- Latency compounds: each hop requires a full generation pass to restate prior context

AMP eliminates the prose layer between agents while preserving full human-readability for audit and debugging.

---

## Design Principles

1. **Embeddings are the primary semantic channel** — intent, context, and results travel as vectors. Prose summaries are optional metadata for human readers, not the authoritative content.

2. **Typed payloads for structured data** — parameters, file paths, goal IDs, draft IDs, CI status, approval decisions, and similar structured values are typed fields, never embedded in prose.

3. **Context hashing eliminates re-transmission** — agents share a context hash registry. If the receiver already has the context (same hash), the sender omits the embedding body and sends only the hash reference.

4. **Every message is an audit event** — the AMP broker logs each message to the TA audit trail with full fidelity. Compliance queries are embedding searches, not log grep.

5. **Degrade gracefully to natural language** — if a receiver doesn't support AMP, the broker serializes the embedding + payload to a prose summary and delivers it as a normal channel message. No message is ever dropped.

---

## Message Format

### Envelope

```json
{
  "amp_version": "1.0",
  "id": "amp-<uuid>",
  "from": "<agent-id>",
  "to": "<agent-id | broadcast>",
  "timestamp": "2026-03-21T15:00:00Z",
  "ttl": 3,

  "intent_embedding": [0.021, -0.134, ...],
  "intent_model": "text-embedding-3-small",
  "intent_dims": 1536,

  "context_hash": "sha256:<hash>",
  "context_embedding": null,

  "payload_type": "command | query | response | event | ack",
  "payload": { ... },

  "signature": "<hmac-sha256-hex | null>"
}
```

**Fields:**

| Field | Required | Description |
|---|---|---|
| `amp_version` | yes | Protocol version |
| `id` | yes | Unique message ID (`amp-` prefix + UUID) |
| `from` | yes | Sending agent ID (registered in AMP broker) |
| `to` | yes | Receiving agent ID or `"broadcast"` |
| `timestamp` | yes | ISO 8601 send time |
| `ttl` | yes | Hop count; broker decrements and drops at 0 |
| `intent_embedding` | yes | Vector encoding of message intent |
| `intent_model` | yes | Embedding model used |
| `context_hash` | yes | SHA-256 of shared context; null if no prior context |
| `context_embedding` | no | Full context vector if receiver may not have it |
| `payload_type` | yes | One of: `command`, `query`, `response`, `event`, `ack` |
| `payload` | yes | Typed struct (see below) |
| `signature` | no | HMAC-SHA256 of canonical message bytes; nil in dev mode |

### Payload Types

#### `command`
Direct instruction from one agent to another.

```json
{
  "action": "implement_feature",
  "parameters": {
    "goal_id": "dc7fe852-...",
    "phase": "v0.13.2",
    "scope_embedding": [0.041, ...],
    "constraints": ["no_new_deps", "must_pass_verify"],
    "priority": "high"
  },
  "approval_required": false,
  "timeout_secs": 3600
}
```

#### `query`
Request for information; expects a `response`.

```json
{
  "question_embedding": [0.089, ...],
  "scope": "codebase | goal | draft | plan",
  "entity_id": "dc7fe852-...",
  "expected_type": "embedding | structured | bool",
  "deadline_secs": 30
}
```

#### `response`
Reply to a prior `query` or `command`.

```json
{
  "to_message_id": "amp-<uuid>",
  "status": "ok | partial | error | deferred",
  "result_embedding": [0.032, ...],
  "structured_result": { ... },
  "error_code": null,
  "error_message": null
}
```

#### `event`
Broadcast notification of state change. No reply expected.

```json
{
  "event_type": "draft_ready | goal_completed | ci_passed | approval_needed | file_changed",
  "entity_id": "<draft-id | goal-id | pr-id>",
  "entity_type": "draft | goal | pr | file",
  "delta_embedding": [0.011, ...],
  "metadata": { "draft_id": "...", "file_count": 12 }
}
```

#### `ack`
Lightweight acknowledgement. No embedding required.

```json
{
  "to_message_id": "amp-<uuid>",
  "status": "received | processing | rejected",
  "reason": null
}
```

---

## Context Hash Registry

The context hash eliminates the most expensive pattern in multi-agent work: re-transmitting the same context at every message boundary.

**How it works:**

1. When an agent first encounters a context (codebase snapshot, prior conversation, goal state), the AMP broker embeds it and registers `sha256(canonical_bytes) → embedding`.

2. Subsequent messages reference the context by hash only. The broker resolves the hash to the stored embedding for the receiver.

3. If the receiver is on a different machine or session, the broker includes the full `context_embedding` in the envelope (cache miss path).

**Estimated savings:** In a 10-message goal run where each agent would otherwise re-embed 4,000 tokens of prior context, context hashing reduces total embedding tokens by ~60–80%.

---

## Integration with Trusted Autonomy

### Architecture

```
┌─────────────────────────────────────────────────────┐
│                   TA Daemon                          │
│                                                      │
│  ┌──────────┐    ┌──────────────┐    ┌───────────┐  │
│  │ Agent A  │───▶│  AMP Broker  │───▶│  Agent B  │  │
│  │(claude)  │    │  (plugin)    │    │ (codex /  │  │
│  └──────────┘    │              │    │  bmad-dev)│  │
│                  │ • route msgs │    └───────────┘  │
│                  │ • ctx cache  │                    │
│                  │ • audit log  │                    │
│                  │ • fallback   │                    │
│                  └──────┬───────┘                    │
│                         │                            │
│                  ┌──────▼───────┐                    │
│                  │  AMP Audit   │                    │
│                  │  Trail       │                    │
│                  │ (.ta/amp/    │                    │
│                  │  messages/)  │                    │
│                  └──────────────┘                    │
└─────────────────────────────────────────────────────┘
```

### Plugin Installation

AMP ships as a TA plugin — declare it in `.ta/project.toml`:

```toml
[plugins.amp]
type    = "broker"
version = ">=0.1.0"
source  = "registry:ta-amp-broker"
```

Or build from source:
```bash
git clone https://github.com/amp-protocol/amp-broker
cd amp-broker && cargo build --release
mkdir -p .ta/plugins/brokers/amp
cp target/release/amp-broker .ta/plugins/brokers/amp/
```

### Configuration

```toml
# .ta/daemon.toml
[amp]
enabled        = true
broker_url     = "unix://.ta/amp.sock"   # or tcp://127.0.0.1:7701
embedding_model = "text-embedding-3-small"
context_cache_mb = 256                   # in-process context hash registry
audit_path     = ".ta/amp/messages"      # append-only JSONL audit log
fallback_to_nl = true                    # deliver as prose if receiver is non-AMP

[amp.auth]
mode = "hmac"                # "none" (dev), "hmac" (local), "mtls" (distributed)
secret_env = "TA_AMP_SECRET" # for hmac mode
```

### Agent Registration

Agents register with the AMP broker on startup. Registration declares:
- Agent ID and capabilities
- Supported payload types
- Embedding model and dimensionality
- Whether the agent can send and/or receive AMP messages

```json
{
  "agent_id": "bmad-architect-01",
  "capabilities": ["design", "architecture", "review"],
  "amp_receive": true,
  "amp_send": true,
  "embedding_model": "text-embedding-3-small",
  "dims": 1536,
  "fallback_nl": false
}
```

### Message Flow in a TA Goal

In a standard TA macro-goal with multiple agents:

```
ta run "v0.13.2" --macro --agents bmad-architect,bmad-dev,bmad-qa

1. Orchestrator → bmad-architect  [AMP command: design_phase]
   intent_embedding: [design intent vector]
   payload: { scope: "MCP transport", constraints: [...] }

2. bmad-architect → Orchestrator  [AMP response]
   result_embedding: [architecture decision vector]
   structured_result: { files: ["docs/architecture.md"], decisions: [...] }

3. Orchestrator → bmad-dev  [AMP command: implement]
   context_hash: <hash of architecture decision>  ← no re-transmission
   payload: { phase: "v0.13.2", scope_embedding: [...] }

4. bmad-dev → bmad-qa  [AMP event: draft_ready]
   entity_id: "draft-abc123"
   delta_embedding: [diff content vector]

5. bmad-qa → Orchestrator  [AMP response: qa_complete]
   status: "ok"
   structured_result: { tests_passed: 47, coverage: 0.91 }
```

Total tokens saved vs. natural language relay: the architecture document (step 2) is never re-transmitted as prose to bmad-dev (step 3) — only the context hash travels.

### Audit Trail

Every AMP message is appended to `.ta/amp/messages/YYYY-MM-DD.jsonl`:

```json
{"timestamp":"2026-03-21T15:01:02Z","id":"amp-abc123","from":"bmad-architect-01","to":"bmad-dev-01","payload_type":"command","intent_preview":"implement MCP transport layer","context_hash":"sha256:deadbeef...","token_estimate":0,"prose_equivalent_estimate":840}
```

The `prose_equivalent_estimate` field records how many tokens the same message would have consumed if sent as natural language, enabling direct measurement of protocol savings.

Query the audit trail:

```bash
ta amp log                                   # recent messages
ta amp log --goal dc7fe852                   # messages for a specific goal
ta amp stats                                 # aggregate token savings
ta amp stats --since 7d                      # last 7 days
```

---

## Measuring Utility and Savings

### Metrics Tracked

| Metric | How measured |
|---|---|
| **Token savings** | `prose_equivalent_estimate - actual_tokens_sent` per message |
| **Latency reduction** | Round-trip time: AMP message vs. equivalent NL exchange |
| **Context re-transmission rate** | `context_hash_hits / total_messages` |
| **Fallback rate** | `fallback_nl_count / total_messages` (lower = more AMP-native agents) |
| **Message clarity score** | Cosine similarity between `intent_embedding` and `result_embedding` (measures whether the response addressed the intent) |

### Reporting

```bash
# Session summary
ta amp stats
# AMP Stats (last 30 days)
# ─────────────────────────────────────────────
# Messages sent:          1,247
# Context hash hits:        891  (71.5%)
# Fallback to NL:            43  (3.4%)
#
# Token savings
#   Estimated NL tokens:  892,400
#   Actual AMP tokens:    124,300
#   Net saved:            768,100  (86.1%)
#   Est. cost saved:        $2.30  (at $3/M tokens)
#
# Avg message latency:      42ms  (vs ~1,800ms NL relay)
# Avg clarity score:        0.94

# Per-goal breakdown
ta amp stats --goal dc7fe852 --verbose
```

### Embedding in TA Reports

`ta plan status` and `ta status` will surface a one-line AMP efficiency indicator when the broker is active:

```
AMP: 1,247 msgs · 86% token savings · 0.94 clarity
```

---

## OSS Model

### Repository Structure

AMP is designed as a standalone open standard with TA as the reference implementation.

```
github.com/amp-protocol/
├── amp-spec/            # Protocol specification (this document, versioned)
│   ├── spec/v1.0.md
│   ├── schemas/         # JSON Schema for all message types
│   └── CHANGELOG.md
│
├── amp-broker/          # Reference broker (Rust)
│   ├── src/
│   │   ├── broker.rs    # Routing, TTL, audit
│   │   ├── context.rs   # Hash registry + embedding cache
│   │   ├── fallback.rs  # NL serialization for non-AMP receivers
│   │   └── metrics.rs   # Token savings tracking
│   └── Cargo.toml
│
├── amp-sdk-rust/        # Rust client library
├── amp-sdk-python/      # Python client (for BMAD, LangChain, etc.)
├── amp-sdk-typescript/  # TypeScript client (for Claude Flow, web agents)
└── amp-conformance/     # Test suite for protocol compliance
```

### Governance

- **Spec versioning**: Semantic versioning. Breaking changes require a major version bump and a 90-day deprecation window.
- **Extension points**: Payload types are extensible. New `payload_type` values are registered via a lightweight RFC process in `amp-spec`.
- **Embedding model agnosticism**: The spec mandates that `intent_model` and `dims` are declared per-message. Brokers must support routing between agents using different embedding models (with a cross-model similarity bridge for context hash resolution).
- **Security tiers**: `none` (local dev), `hmac` (single-machine or trusted LAN), `mtls` (distributed / cloud).

### Integration Points Beyond TA

AMP is designed to be adopted by any multi-agent framework:

| Framework | Integration path |
|---|---|
| **Claude Flow** | `amp-sdk-typescript` — swarm agents register with AMP broker, coordinate via events |
| **BMAD** | `amp-sdk-python` — PM/architect/dev/QA roles send typed handoffs |
| **LangChain / LangGraph** | `amp-sdk-python` — graph edges become AMP messages with full audit |
| **AutoGen** | `amp-sdk-python` — agent conversations mediated by AMP broker |
| **Custom** | HTTP/WebSocket API on the broker — any language, no SDK required |

### Versioning and TA Phase

AMP broker development is tracked in PLAN.md:

| Phase | Content |
|---|---|
| v0.13.2 (current) | MCP transport abstraction (foundation for broker socket layer) |
| v0.14.x | AMP broker alpha — local broker, context hash registry, audit trail |
| v0.15.x | AMP SDK releases — Rust, Python, TypeScript |
| v0.16.x | Cross-model context bridge, distributed broker (mTLS), conformance suite |

---

## Example: Goal Handoff Without Natural Language

**Before AMP** (natural language relay, ~1,200 tokens per hop):
```
Orchestrator → Agent:
"You are implementing phase v0.13.2 of the Trusted Autonomy project. The
architecture document at docs/architecture.md specifies that the MCP transport
layer should support both TCP sockets and Unix domain sockets. The current
implementation only supports stdio. You need to add a TransportAdapter trait..."
[800 tokens of context re-stated from prior messages]
```

**With AMP** (~40 tokens equivalent):
```json
{
  "payload_type": "command",
  "intent_embedding": [...],          // "implement transport abstraction"
  "context_hash": "sha256:abc123",    // receiver already has the architecture doc
  "payload": {
    "action": "implement",
    "phase": "v0.13.2",
    "scope": ["crates/ta-mcp/src/transport.rs"],
    "constraints": ["trait_based", "backward_compatible"],
    "deadline_secs": 3600
  }
}
```

The receiver retrieves the architecture document from its local context cache using the hash. No re-transmission. No re-tokenization.

---

## FAQ

**Q: Does AMP require all agents to support it?**
No. The broker's `fallback_nl = true` default serializes AMP messages to a prose summary for non-AMP receivers. Adoption is incremental.

**Q: What embedding model should I use?**
The broker is model-agnostic. `text-embedding-3-small` (1536 dims) is the default for cloud deployments. For local/offline use, `nomic-embed-text` via Ollama works well. The broker handles cross-model routing via a similarity bridge.

**Q: How is this different from function calling / tool use?**
Tool use is for agent-to-tool communication (structured I/O). AMP is for agent-to-agent communication where the semantic content itself needs to travel efficiently. AMP messages can carry tool-call results as structured payload fields.

**Q: What prevents a malicious agent from impersonating another?**
In `hmac` mode, every message is signed with a shared secret. In `mtls` mode, each agent has a certificate. In `none` mode (local dev only), no authentication is enforced — acceptable when all agents run in the same process.

**Q: Can I query the AMP audit trail for compliance reporting?**
Yes. The JSONL audit log is queryable with `ta amp log` and exportable for external SIEM systems. Each entry includes the intent embedding — you can cluster messages by semantic content to identify communication patterns and anomalies.

---

## Prior Art and Protocol Landscape

AMP is not designed in a vacuum. This section maps AMP against every significant agent communication protocol — from the 1990s academic lineage through the 2025 enterprise standards — identifying what each got right, what AMP inherits, and where it diverges.

### The Academic Lineage (1992–2005)

#### KQML (1992)

Emerged from DARPA's Knowledge Sharing Effort. The first serious attempt at a standardized agent communication language. Messages are Lisp-style S-expressions with a performative (speech act), sender/receiver, and an open content layer:

```
(ask-one
  :sender agent-a
  :receiver agent-b
  :language KIF
  :ontology stock-ontology
  :content (price IBM ?x))
```

**What KQML got right:** Speech acts as the organizing primitive — the distinction between `tell`, `ask`, `subscribe`, `achieve` maps directly to AMP's payload types. Facilitator agents (message routers) are a first-class architecture element, the direct ancestor of AMP's broker.

**Why it failed:** No formal semantics — every implementor defined performatives differently, making true interoperability impossible. No security, no binary encoding, no transport standard. DARPA funding ended; no standards body continued it.

**AMP inheritance:** The performative taxonomy (command ≈ `achieve`, query ≈ `ask-one`, event ≈ `subscribe` notification, ack ≈ `ready`). The broker-as-first-class-infrastructure pattern.

---

#### FIPA ACL (1996–2002)

The Foundation for Intelligent Physical Agents (an IEEE standards body) formalized KQML's ideas with genuine rigor. Added modal-logic formal semantics (the Semantic Language, SL), 22 precisely defined performatives, three wire encodings (string, XML, bit-efficient binary), and a full platform model (AMS, DF, ACC).

```
(inform
  :sender (agent-identifier :name j@foo.com)
  :receiver (set (agent-identifier :name i@bar.com))
  :content (price share 10)
  :ontology stock-ontology
  :language fipa-sl
  :conversation-id cid-1234)
```

JADE (the Java reference implementation, still maintained as LGPL) made FIPA ACL actually deployable at enterprise scale with a sniffer agent for message monitoring and a full directory facilitator.

**What FIPA ACL got right:** Formal interaction protocols (Contract Net, English Auction, Subscribe) as first-class entities — these are the protocol-level equivalent of AMP's expected conversation flows. Conversation-ID threading for audit. The three-tier wire encoding approach (human-readable, XML, binary) is exactly AMP's strategy.

**Why it failed:** Formal semantics (SL modal logic) were too complex for most implementors — compliance was notional, not verified. The platform model (central AMS/DF) doesn't scale beyond a controlled deployment. Standards body dissolved 2005; no LLM integration path. Still in production in some defense/telecom/smart-grid deployments.

**AMP inheritance:** Conversation threading via message IDs. Multiple wire encoding tiers. Formal interaction protocol templates. The recognition that a facilitator/broker is a necessary infrastructure component, not an optional add-on.

---

#### AgentSpeak / Jason (1996/2005)

A BDI (Beliefs-Desires-Intentions) agent programming language where inter-agent communication is a language primitive:

```
.send(agent-b, achieve, analyze(market))
.send(agent-c, askOne, temperature(X), Reply)
```

Not a wire protocol — a language+runtime. Still actively used in academic MAS research, formal verification, and robotics simulation. Bridges to JADE for FIPA compliance.

**AMP relevance:** The BDI model's clean separation of state (beliefs), goals (desires), and execution plans (intentions) is a useful mental model for AMP's `query`, `command`, and `response` payload taxonomy.

---

### The Research Wave (2024)

#### DroidSpeak (Microsoft Research / U. Chicago, Nov 2024)

A research paper (arXiv:2411.02820) proposing transmission of intermediate neural representations — KV-cache tensors and token embedding caches — between LLMs instead of re-tokenizing shared context. Measured 2.78x prefill speedup with negligible accuracy loss on benchmarks like HotPotQA and GSM8K.

**What it gets right:** The core insight is identical to AMP's context hash registry — the most expensive operation in multi-agent pipelines is re-encoding shared context from scratch. DroidSpeak eliminates this at the GPU layer.

**Why it's not a protocol:** Requires homogeneous model families (same tokenizer, same layer dimensions — a Llama-3-8B base and its fine-tuned variants, but not two different model families). Requires InfiniBand-class interconnects. Zero human observability — tensor representations are opaque blobs. Not deployable as an open standard as of 2026.

**AMP's approach to the same problem:** Context hashing operates at the semantic layer (embeddings, not raw tensors), is model-agnostic, uses cross-model similarity bridges where needed, and produces human-readable audit trails. Slower than DroidSpeak's GPU-level transfer but deployable across heterogeneous agent ecosystems without shared model weights.

#### CIPHER (ICLR 2024)

Research demonstrating agents communicating via compressed embedding vectors rather than natural language. Showed token efficiency gains but faced the same model homogeneity and observability barriers as DroidSpeak.

**AMP relevance:** Empirical evidence that embedding-based communication is measurably more efficient. AMP adopts the embedding-as-primary-channel principle but adds the typed payload wrapper that makes messages human-interpretable for audit.

---

### The Enterprise Standard Wave (2024–2025)

#### Model Context Protocol — MCP (Anthropic, Nov 2024)

JSON-RPC 2.0 over stdio or HTTP+SSE. The dominant LLM-tool integration protocol as of 2026. Defines three primitives: **resources** (data the model can read), **tools** (actions the model can invoke), **prompts** (templates). Added **sampling** (tool server triggers model calls) and **elicitation** (server requests user input) in 2025.

TA already uses MCP as its primary tool integration layer. v0.13.2 (in progress) adds TCP/Unix socket transport support.

**Relationship to AMP:** MCP is vertical (one LLM connecting to many tools). AMP is horizontal (peer agents communicating). These are complementary layers, not competitors. An AMP message can carry a tool-call result as a structured payload. The AMP broker can be implemented as an MCP server, making AMP-capable agents available to any MCP client. The MCP transport abstraction being built in v0.13.2 is the foundation the AMP broker will use.

**Governance:** Linux Foundation (AAIF — Agent AI Integration Framework). Apache 2.0.

---

#### Agent-to-Agent Protocol — A2A (Google, Apr 2025)

JSON-RPC 2.0 over HTTP+SSE (with gRPC support). Agent discovery via `/.well-known/agent.json` (no central registry required). Task lifecycle: `working → input-required → completed/failed`. Message parts: `text`, `file`, `data` (typed structured JSON). Strong security: DID-based identity, OAuth2, mutual TLS. 50+ organizations at launch; broad enterprise adoption including Cisco, SAP, Salesforce. Linux Foundation governance.

**Relationship to AMP:** A2A is the closest existing protocol to what AMP targets, and the most important interoperability target. Key differences:

| Dimension | A2A | AMP |
|---|---|---|
| Semantic channel | Natural language (`text` parts) | Intent embeddings + typed payloads |
| Context re-transmission | Full re-send every message | Context hash eliminates re-send |
| Auditability | Task IDs + timestamps | Task IDs + timestamps + intent embeddings (cluster-queryable) |
| Discovery | HTTP crawl (well-known endpoint) | Broker registry + capability embeddings |
| Embedding-native | No | Yes |
| Token efficiency | Baseline | 60–86% reduction (measured) |

**AMP's A2A compatibility strategy:** AMP messages can be wrapped in A2A `data` parts for delivery to A2A agents that don't natively support AMP. The AMP broker can expose an A2A-compatible `/.well-known/agent.json` endpoint. Long term, AMP proposes adding an `embedding` part kind to A2A — the broker would submit this as an extension proposal to the Linux Foundation A2A working group.

---

#### ACP — Agent Communication Protocol (IBM Research / BeeAI, 2024)

HTTP-REST native. Core abstraction is a **Run** — a stateful invocation of an agent. Messages are multipart MIME (text/plain, image/png, application/json, etc.). Run states: `created → in_progress → awaiting → completed/failed`. OTLP instrumented for OpenTelemetry.

Note: As of 2025, ACP is being evaluated for convergence with A2A under the Linux Foundation.

**Relationship to AMP:** ACP's multipart MIME content model is the cleanest existing approach to multimodal messages. AMP adopts a similar part-based envelope but adds the intent embedding as a first-class field rather than an optional content part. The OTLP observability story in ACP is the model for AMP's `ta amp stats` reporting.

---

#### NLIP — Natural Language Interaction Protocol (Ecma TC-56, Dec 2025)

The first formally standardized LLM-era agent communication protocol from a recognized international standards body (Ecma International, which also standardizes JavaScript). Five specifications: ECMA-430 (core format), ECMA-431 (HTTP binding), ECMA-432 (WebSocket + CBOR binary), ECMA-433 (AMQP), ECMA-434 (three-tier security including prompt injection prevention).

```json
{
  "nlip-version": "1.0",
  "message-id": "msg-uuid",
  "sender": {"id": "agent-a", "type": "agent"},
  "content": [
    {"type": "text", "value": "Process this document"},
    {"type": "binary", "mime-type": "application/pdf", "data": "..."}
  ]
}
```

**Relationship to AMP:** NLIP is the most aligned existing standard to AMP's goals. Its multi-transport approach (HTTP, WebSocket/CBOR, AMQP) maps directly to AMP's planned transport tiers. ECMA-434's security profiles — particularly the prompt injection prevention tier — are a direct model for AMP's security roadmap. AMP should pursue NLIP compatibility as a primary goal: an AMP message should be expressible as an NLIP message with an additional `embedding` content type.

**Adoption caveat:** Published December 2025, real-world adoption is nascent. The standards-body pace is slower than GitHub-driven protocols, but long-term enterprise compliance requirements favor formal standards. AMP should engage with TC-56 to propose an `embedding` content type extension.

---

#### AGNTCY / SLIM (Mar 2025)

AGNTCY is a Linux Foundation initiative (backed by Cisco) with SLIM (Secure Language Interoperability and Messaging) as its wire format. Post-quantum cryptography (CRYSTALS-Kyber for key exchange, CRYSTALS-Dilithium for signing), binary encoding, access-event logging. Targets regulated enterprise environments where quantum-safe security is a compliance requirement.

**Relationship to AMP:** SLIM's quantum-safe signing approach is the long-term model for AMP's mTLS tier upgrade path. As quantum-safe cryptography becomes a compliance requirement (likely mid-2020s), AMP should provide a SLIM-compatible transport binding.

---

#### ANP — Agent Network Protocol (2024–2025)

W3C Community Group backed. Uses JSON-LD (Linked Data) for agent identity and capability description. Targets fully decentralized internet-scale agent networks where no central registry is trusted. Cryptographic message signing is mandatory. Each agent has a W3C DID (Decentralized Identifier).

**Relationship to AMP:** ANP's DID-based identity model is the right long-term approach for AMP's distributed deployment scenarios. AMP's current HMAC/mTLS auth is appropriate for single-organization or trusted-LAN deployments. For internet-scale federated deployments, AMP should adopt DID-based identity.

---

#### LangGraph / CrewAI / AutoGen

These are frameworks, not wire protocols. Their agent communication patterns (shared state machines, task delegation, pub-sub) are relevant for how agents built on these frameworks would integrate with AMP:

- **LangGraph**: typed dict state + checkpointing. AMP messages map to graph edge transitions; the AMP broker acts as the shared state store for cross-graph coordination.
- **AutoGen**: Pydantic message types + async pub-sub. AMP's typed payloads are a natural extension — AutoGen messages become AMP `command` and `response` payloads.
- **CrewAI**: role-based delegation. AMP `command` payloads replace the untyped string `context` parameter in CrewAI's `DelegateWorkTool`, eliminating the "delegation ping-pong" bug caused by LLMs passing dicts where strings are expected.

---

### Positioning Summary

```
                    ←— Vertical (LLM ↔ tools) ——— Horizontal (agent ↔ agent) →

Widely             MCP ●                                              A2A ●
deployed           (tool integration)                          (enterprise agents)

Gaining            ACP ●               NLIP ●
traction           (IBM/BeeAI)         (Ecma standard)

Research /         DroidSpeak ●     CIPHER ●          ANP ●
early              (GPU tensors)    (embedding vecs)   (decentralized)

                        ← text/NL ——— typed payloads ——— embedding-native →

AMP targets: horizontal peer communication, typed payloads, embedding-native
intent channel, auditable by design, compatible with A2A and NLIP.
```

**AMP's unique position:** Every production protocol today uses natural language or typed-but-text JSON as the primary semantic channel. AMP is the first production-grade protocol designed around embeddings as the primary semantic channel — with typed structured payloads for precision, context hashing for efficiency, and human-readable audit trails for compliance. It is not the first to propose this (CIPHER, DroidSpeak), but it is the first to make it deployable without model homogeneity constraints, and the first to couple it with a full compliance audit story.

---

### Interoperability Options

AMP supports three integration modes for working alongside existing protocols:

#### Option 1: AMP as an A2A Extension (recommended for enterprise)

AMP messages travel inside A2A `data` parts. A2A agents that don't understand AMP receive a prose-fallback `text` part alongside the `data` part. AMP-native agents use the `data` part directly. This requires zero changes to A2A infrastructure.

```json
{
  "role": "agent",
  "parts": [
    {
      "kind": "data",
      "data": {
        "amp_version": "1.0",
        "payload_type": "command",
        "intent_embedding": [...],
        "payload": { "action": "implement", "phase": "v0.13.2" }
      }
    },
    {
      "kind": "text",
      "text": "Please implement the MCP transport abstraction for phase v0.13.2."
    }
  ]
}
```

Long-term goal: propose `embedding` as a formal A2A part kind through the Linux Foundation working group.

#### Option 2: AMP over NLIP (recommended for regulated environments)

AMP's intent embedding maps to a new `embedding` content type in the NLIP message format. AMP plans to submit this as an extension proposal to Ecma TC-56. Until ratified, AMP embeds the intent vector in an `application/amp+json` binary content part.

#### Option 3: AMP as standalone (recommended for TA-native deployments)

AMP runs as the native broker for all agent-to-agent communication within a TA deployment. MCP handles agent-to-tool communication (vertical). A2A handles federation with external agents (horizontal, text-based). AMP handles internal multi-agent coordination (horizontal, embedding-native).

```
External agents ←— A2A ——→ TA AMP Broker ←— AMP ——→ Internal agents
                                    ↕
                               MCP (tools)
```

---

## Auto-Optimization and User-Defined Routes

### Automatic Path Optimization

AMP's broker learns which communication paths are most efficient and automatically optimizes routing without user intervention.

**Path scoring:** Each agent-to-agent communication channel is scored on three dimensions:
- **Token efficiency**: `prose_equivalent_tokens / actual_amp_tokens` (higher = better)
- **Clarity score**: cosine similarity between `intent_embedding` and `result_embedding` (measures whether the response addressed the intent; > 0.85 is good)
- **Latency**: round-trip time for the message type

After 10+ exchanges on a given path, the broker builds a **route profile** that predicts the optimal encoding strategy (full embedding, hash-only, typed payload without embedding, or prose fallback) for new messages on that path.

```toml
# .ta/daemon.toml
[amp.optimization]
enabled        = true
min_samples    = 10      # exchanges before optimization kicks in
clarity_floor  = 0.80    # fall back to NL if clarity drops below this
token_budget   = 500     # max tokens per message before forcing embedding-only
auto_register  = true    # auto-register new agent paths when discovered
```

**Automatic fallback escalation:** If a path's clarity score drops below `clarity_floor` for 3 consecutive messages, the broker automatically escalates to prose for that path until the user reviews it. This prevents silent degradation where agents technically communicate but misunderstand each other.

### User-Defined Routes for Novel Paths

For workflows that don't match learned patterns — new agent types, experimental coordination, non-standard handoff sequences — users define explicit routes in `.ta/amp-routes.toml`:

```toml
# .ta/amp-routes.toml

# Custom route: bmad-architect sends design documents to a Perforce review agent
[[routes]]
from        = "bmad-architect"
to          = "perforce-review-agent"
payload_type = "command"
encoding    = "embedding+typed"      # force embedding + typed payload (no prose)
context_strategy = "hash_only"       # never re-send context, always hash
timeout_secs = 120
retry_on_clarity_below = 0.75        # retry with prose if unclear

# Custom route: broadcast CI results to all active bmad agents
[[routes]]
from        = "ci-watcher"
to          = "broadcast"
filter_type = "event"
filter_event_types = ["ci_passed", "ci_failed"]
encoding    = "typed_only"           # no embeddings needed for boolean CI events
fan_out_max = 8                      # max concurrent deliveries

# Pass-through: forward all goal_completed events to external A2A endpoint
[[routes]]
from        = "*"
to          = "external-a2a://agent.partner.com"
filter_event_types = ["goal_completed", "draft_ready"]
encoding    = "a2a_compat"          # wrap in A2A data part
```

Define and manage routes:

```bash
ta amp routes list                     # show all routes (learned + user-defined)
ta amp routes add --from bmad-dev --to bmad-qa --encoding embedding+typed
ta amp routes test <route-name>        # dry-run with a sample message
ta amp routes disable <route-name>     # disable without deleting
ta amp routes stats <route-name>       # efficiency metrics for this route
```

---

## Memory System Integration

### Architecture Decision: Integrated vs. Standalone

**Question**: Should AMP use TA's memory system (with RuVector as the backing store) or define its own standalone memory abstraction?

**Recommendation: integrated via abstraction interface.**

The memory system's core operations for AMP are:
1. **Context hash registry**: store and retrieve embeddings by content hash
2. **Route profile learning**: persist per-path efficiency statistics across sessions
3. **Intent clustering**: group past messages by semantic similarity for audit queries

These are exactly what the memory system does. Standing up a separate memory store for AMP would mean maintaining two vector stores, two eviction policies, and two backup paths. The cost is not justified.

However, AMP should not depend directly on TA's memory implementation — it should depend on an **interface**:

```rust
// crates/ta-amp/src/memory.rs
pub trait AmpMemory: Send + Sync {
    fn store_context(&self, hash: &str, embedding: &[f32]) -> anyhow::Result<()>;
    fn retrieve_context(&self, hash: &str) -> anyhow::Result<Option<Vec<f32>>>;
    fn search_similar(&self, embedding: &[f32], top_k: usize) -> anyhow::Result<Vec<(String, f32)>>;
    fn store_route_profile(&self, route_id: &str, profile: &RouteProfile) -> anyhow::Result<()>;
    fn load_route_profile(&self, route_id: &str) -> anyhow::Result<Option<RouteProfile>>;
}
```

The default implementation in TA wires this to the existing memory crate (which backs to RuVector when available, SQLite otherwise). An external party building on the AMP spec wires it to their own store. This keeps AMP portable without duplicating infrastructure.

**Is a memory abstraction necessary?** For the OSS protocol spec: yes, it's necessary. AMP would be non-portable if it hardcoded TA's memory layer. For TA's own deployment: the abstraction is a thin wrapper — one adapter implementation that delegates to the existing store. The overhead is ~50 lines of code and zero runtime cost.

### RuVector as the Default Backend

When RuVector is available (as defined in the v0.14.x memory unification plan), the `AmpMemory` implementation uses it for:
- **HNSW index** for context embedding lookup (O(log n) approximate nearest neighbor — fast enough for real-time routing decisions)
- **Exact hash lookup** via a secondary key-value index (not approximate — hash matches must be exact)
- **Route profile persistence** via RuVector's structured document storage

When RuVector is not available (minimal deployments, CI), the adapter falls back to an in-process `HashMap<String, Vec<f32>>` with no persistence across daemon restarts. Context hash hits are 0% cold start, rising as the broker warms up.

### Memory Lifecycle

Context embeddings accumulate. A codebase with active multi-agent development can generate thousands of context entries per day. The memory system applies the same compaction rules as goal history:

```toml
[amp.memory]
context_ttl_days   = 30    # evict context embeddings not referenced in 30 days
route_profile_days = 90    # keep route profiles for 90 days (longer — these are learned)
max_context_mb     = 256   # hard cap on context cache size
```

`ta gc` respects these settings and reports bytes reclaimed from AMP context cache alongside goal staging cleanup.

---

## Implementation Phase

### When to Implement

AMP has dependencies on two infrastructure items that must land first:

| Dependency | Phase | Status |
|---|---|---|
| MCP transport abstraction (TCP/Unix socket) | v0.13.2 | In progress |
| Memory system unification + RuVector integration | v0.14.x | Pending |

Given these dependencies, AMP implementation phases:

| Phase | Version | Content |
|---|---|---|
| **Foundation** | v0.13.2 | MCP transport layer (broker will use this socket infrastructure) |
| **Spec finalization** | v0.13.9–v0.14.0 | AMP v1.0 spec published as OSS; community feedback period |
| **Broker alpha** | v0.14.1 | Local AMP broker: routing, context hash registry (in-memory), JSONL audit log |
| **Memory integration** | v0.14.2 | Wire `AmpMemory` to RuVector; persistent context cache; route learning |
| **Auto-optimization** | v0.14.3 | Route profiles, clarity scoring, auto-fallback escalation, `ta amp stats` |
| **A2A compatibility** | v0.15.0 | A2A `data` part wrapper; AMP broker exposes A2A-compatible endpoint |
| **NLIP extension proposal** | v0.15.x | Submit `embedding` content type to Ecma TC-56 |
| **SDK releases** | v0.15.x | `amp-sdk-rust`, `amp-sdk-python`, `amp-sdk-typescript` published |
| **Distributed broker** | v0.16.x | mTLS, DID-based identity, quantum-safe signing (SLIM-compatible) |
| **Conformance suite** | v0.16.x | `amp-conformance` test suite; engage Linux Foundation for A2A working group |

**Why not sooner?** The context hash registry without a persistent vector store is useful but cold-starts every time the daemon restarts. The real efficiency gains compound only when the route profiles survive across sessions — which requires the RuVector integration. Starting the spec now (v0.13.9–v0.14.0) lets the community shape it before the implementation commits the design.
