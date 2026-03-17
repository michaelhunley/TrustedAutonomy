# Rationale: Moving Trusted Autonomy to Local Inference

## Executive Summary

Trusted Autonomy's core thesis is that all agent actions must be mediated
through staging, policy, and audit — a local-first substrate for safe AI
agent autonomy. Today, the agent's *reasoning* still happens on remote
servers owned by third parties. This document argues that local inference
is the natural and necessary completion of TA's architectural promise:
if we don't control the reasoning layer, we don't control autonomy.

---

## 1. Alignment with Core Architecture

TA already enforces a local-first model for *actions*. The overlay
workspace, draft review pipeline, and policy engine all run on the
user's machine. But the agent's decisions — the most consequential
part of the loop — are formed on infrastructure we neither own nor
audit. This is an architectural asymmetry. The staging layer protects
against bad *outputs*, but it cannot protect against:

- Silent changes to the model's behavior via provider-side updates.
- Inference-time data collection that undermines user privacy.
- Service degradation, rate limiting, or discontinuation.
- Latency spikes that break interactive development workflows.

Local inference closes this gap. The entire autonomy loop — from goal
decomposition through reasoning through action staging through review —
runs on hardware the user controls. TA becomes a truly self-contained
system rather than a safety wrapper around someone else's service.

## 2. Privacy and Data Sovereignty

Every API call to a cloud inference provider transmits the user's:

- Source code (often proprietary or pre-publication research).
- File system structure and project metadata.
- Error messages, stack traces, and runtime state.
- Natural language goals that reveal business intent.
- Memory context that accumulates personal and project history.

Even with providers who commit to not training on API data, the data
*transits* infrastructure outside the user's control. It is subject to
legal jurisdiction, subpoena, and breach risk that the user cannot
mitigate. For users in regulated industries — defense, healthcare,
finance, legal — this is often a hard blocker, not a preference.

Local inference eliminates the data exfiltration surface entirely.
Code never leaves the machine. Goals never leave the machine. Memory
never leaves the machine. This is not a privacy policy — it is a
physics guarantee.

## 3. Latency and Developer Experience

Interactive agent workflows are latency-sensitive. The `ta dev` loop
involves rapid iteration: the agent reads code, proposes changes, the
user reviews, the agent adjusts. Round-trip latency to cloud APIs
introduces friction at every step:

- Token generation at 50-80 tokens/second over network vs. 20-40
  tokens/second locally — but local has zero network overhead.
- Cloud API p99 latency includes queue time, cold starts, and
  congestion-driven throttling that local inference never encounters.
- Streaming responses over the network introduce jitter that makes
  the TUI feel inconsistent. Local streaming is deterministic.

For small-to-medium context windows (under 8K tokens), which represent
the majority of TA's agent interactions, a well-quantized local model
on modern hardware (Apple Silicon, RTX 4090, MI300X) matches or beats
cloud API wall-clock time because it eliminates the network entirely.

## 4. Cost Structure

Cloud inference pricing creates a direct, ongoing operational cost
that scales with usage. For an active developer running TA throughout
a workday:

- ~500K tokens/day input, ~200K tokens/day output is typical.
- At current Sonnet-class pricing, this is $5-15/day per developer.
- Teams of 10 developers: $50-150/day, $1,500-4,500/month.
- Heavy agentic workflows (swarms, multi-step goals) multiply this.

Local inference has a one-time hardware cost that amortizes to zero:

- Apple M4 Max (128GB): handles 70B-class models comfortably.
- A single RTX 4090 (24GB VRAM): runs quantized 70B models.
- The hardware serves every other purpose on the developer's desk.

After 3-6 months of active use, local inference is cheaper than cloud
for any individual developer. For teams, the crossover is faster.

## 5. Reliability and Availability

Cloud inference is a dependency with its own failure modes:

- API outages block all agent work. TA cannot stage what it cannot run.
- Rate limits throttle throughput during peak hours — exactly when
  developers need agents most.
- Model deprecation forces migration on the provider's schedule.
- Network partitions (airplane, VPN, air-gapped environments) make
  the agent completely unavailable.

Local inference has exactly one dependency: the user's hardware being
powered on. It works offline. It works on airplanes. It works in
SCIFs. It works during cloud provider incidents. For a tool whose
value proposition is *trusted* autonomy, this reliability matters.

## 6. Model Selection Freedom

Cloud API access limits users to models the provider chooses to serve.
Local inference opens the full ecosystem:

- **Open-weight frontier models**: Llama 3.x (405B, 70B, 8B),
  Mistral Large, Qwen 2.5, DeepSeek V3, Command R+.
- **Specialized fine-tunes**: Code-specific models (DeepSeek Coder,
  CodeQwen), reasoning models, domain-adapted variants.
- **Custom fine-tunes**: Users can fine-tune on their own codebase,
  their own conventions, their own domain language.
- **Quantization tradeoffs**: Users choose their own quality/speed
  balance — Q8 for maximum quality, Q4 for speed, Q2 for fitting
  larger models on constrained hardware.

This is especially important for TA's active memory injection system.
A locally fine-tuned model that has internalized a project's patterns
may outperform a larger cloud model that receives those patterns only
as context window injections.

## 7. Auditability and Reproducibility

TA's audit log records what the agent did, but not *why* — because
the reasoning happens on opaque remote infrastructure. With local
inference:

- The exact model weights are known and versioned.
- Inference parameters (temperature, top-p, seed) are controlled.
- The same input produces the same output (deterministic decoding).
- The full reasoning trace can be captured and stored locally.
- Audit chains can include model identity as a verified field.

This transforms TA's audit capability from "what happened" to "what
happened, why, and with what reasoning process" — a qualitative leap
in accountability that matters for compliance and trust.

## 8. Security Surface Reduction

Every external API integration is an attack surface:

- API keys can be leaked, stolen, or phished.
- Man-in-the-middle attacks on API traffic can exfiltrate code.
- Compromised API endpoints can return adversarial completions.
- Supply-chain attacks on SDK dependencies affect all users.

Local inference eliminates the network attack surface for the
reasoning layer entirely. There is no API key to steal. There is no
network traffic to intercept. There is no remote endpoint to
compromise. The threat model simplifies dramatically.

## 9. Implementation Path

TA's architecture already supports this transition cleanly:

1. **Agent abstraction layer**: TA treats the agent as a black box
   that works in a staging directory. The agent's inference backend
   is already decoupled from TA's orchestration.

2. **YAML agent configs** (`agents/` directory): Already define agent
   parameters. Adding `backend: local` and `model_path:` fields is
   a natural extension of the existing configuration schema.

3. **llama.cpp / vLLM / Ollama**: Mature local inference servers that
   expose OpenAI-compatible APIs. TA can target the same API surface
   whether the backend is local or remote — no protocol changes.

4. **Incremental adoption**: Users can start with cloud inference and
   switch to local as hardware and models improve. TA supports both
   simultaneously — different goals can use different backends.

5. **Model management**: `ta model pull`, `ta model list` commands
   that manage local model weights, similar to Ollama's UX but
   integrated into TA's existing CLI structure.

## 10. Current Hardware Landscape

The hardware argument against local inference has weakened sharply:

- **Apple M4 Max/Ultra**: 128-192GB unified memory, sufficient for
  unquantized 70B models. Memory bandwidth (~800 GB/s) enables
  competitive token generation speed.
- **NVIDIA RTX 5090**: 32GB VRAM, sufficient for Q4 70B models.
  Multi-GPU setups (2x5090) handle larger models.
- **AMD MI300X**: 192GB HBM3, designed for inference workloads.
  Available in workstation form factors.
- **Quantization advances**: GGUF Q4_K_M produces negligible quality
  loss on coding tasks compared to FP16 on most benchmarks.

A developer's existing workstation — the machine already on their
desk — is increasingly sufficient for production-quality local
inference on models that match cloud API capability for code tasks.

## 11. Open-Weight Model Quality

The quality gap between open-weight and proprietary models has
narrowed dramatically for code-centric tasks:

- DeepSeek V3 matches or exceeds GPT-4 on HumanEval, MBPP, and
  SWE-bench benchmarks.
- Qwen 2.5 Coder 32B outperforms many larger models on code
  completion and repair tasks.
- Llama 3.3 70B is competitive with Claude Sonnet on agentic
  coding benchmarks.

For TA's use case — reading code, proposing edits, running commands,
interpreting test output — a well-chosen 70B open-weight model
delivers sufficient capability for the vast majority of tasks.

## 12. What We Lose (and Mitigations)

Honesty requires acknowledging tradeoffs:

- **Frontier capability**: The largest cloud models (Opus-class, o3)
  still outperform open-weight models on the hardest reasoning tasks.
  *Mitigation*: Hybrid mode — local for routine work, cloud API for
  complex goals that exceed local model capability.

- **Context window**: Cloud models offer 128K-1M token contexts.
  Local models are typically limited to 32K-128K.
  *Mitigation*: TA's memory injection and context management already
  work to keep context focused and small. Long context is often a
  crutch for poor context selection.

- **Maintenance burden**: Users must manage model weights, updates,
  and hardware compatibility.
  *Mitigation*: `ta model` commands automate this. Ollama has proven
  that model management can be as simple as `pull` and `run`.

- **Power consumption**: Local inference on GPU is power-hungry.
  *Mitigation*: Apple Silicon inference is remarkably efficient.
  And the power is spent on *your* machine doing *your* work,
  not in a data center doing everyone's work.

## 13. Strategic Independence

Depending on a single cloud provider for the reasoning layer creates
existential risk for any tool built on top of it:

- Pricing changes can make the tool uneconomical overnight.
- Terms of service changes can restrict use cases.
- Model deprecation forces emergency migration.
- Provider acquisition or shutdown eliminates the capability entirely.

Local inference, built on open-weight models and open-source inference
engines, is immune to all of these. TA's value proposition — trusted,
auditable, user-controlled autonomy — should not have a single point
of failure in a third party's business decisions.

## 14. Community and Ecosystem Momentum

The local inference ecosystem is accelerating, not plateauing:

- llama.cpp adds hardware backends and optimizations weekly.
- vLLM and SGLang push serving efficiency forward continuously.
- Every major AI lab now releases open-weight model variants.
- Apple, NVIDIA, AMD, and Intel all invest in on-device inference.
- The MLX ecosystem (Apple Silicon native) is maturing rapidly.

Building on this ecosystem means TA benefits from improvements made
by thousands of contributors across dozens of organizations, none of
whom can unilaterally change the terms of access.

## Conclusion

Trusted Autonomy's mission is to give users safe, auditable control
over AI agent autonomy. Local inference is not an optimization or a
nice-to-have — it is the logical completion of that mission. As long
as the reasoning layer runs on someone else's infrastructure, the
"trusted" in Trusted Autonomy has an asterisk.

The hardware is ready. The models are ready. The inference engines are
ready. TA's architecture already accommodates the transition. The
question is not whether to move to local inference, but how quickly
we can make it the default experience — with cloud inference as the
fallback, not the other way around.
