# Use Case: Continuous Department Agent

> A long-lived "department" that is given a remit and continuously works a goal,
> responding to events and suggesting strategy shifts — all mediated through TA's
> staging-review-apply loop.

---

## Concrete Example: Developer Relations Department

A DevRel department agent is responsible for:
- Monitoring GitHub issues, discussions, and social mentions
- Drafting responses, blog posts, and changelog entries
- Suggesting documentation improvements based on common questions
- Flagging critical issues that need human escalation

The department runs continuously. It produces **drafts** (not final output) that
a human reviews and approves before anything goes public.

---

## How It Works with TA (Phase by Phase)

### Today (v0.4.x) — Manual Loop

The department can operate today as a **human-driven loop** using existing TA commands:

```
Human kicks off goal ──→ Agent works in staging ──→ Draft built
         ↑                                              │
         │                                              ▼
         │                                    Human reviews draft
         │                                              │
         │            ┌────────────────────────────────┬┘
         │            ▼                                ▼
         │     Approve + Apply                   Discuss items
         │            │                                │
         │            ▼                                ▼
         └──── ta run --follow-up ◄───── corrections needed
```

```bash
# 1. Start the department's first work cycle
ta run "DevRel: triage this week's GitHub issues and draft responses" \
  --source ./devrel-workspace --agent claude-code

# 2. Agent works in staging, exits, draft auto-builds
ta draft view <id> --detail medium

# 3. Human reviews — approve some, discuss others
ta draft apply <id> --approve "responses/**" --discuss "blog/**"

# 4. Follow-up for discussed items
ta run "DevRel: revise blog draft per review feedback" \
  --source ./devrel-workspace --follow-up

# 5. Repeat indefinitely
```

**What works**: Full audit trail, selective approval, follow-up context injection.
**What's manual**: Human must kick off each cycle, copy-paste IDs, remember to follow up.

---

### v0.4.1 — Macro Goals (Planned)

Macro goals keep the agent in-session across multiple checkpoints instead of
exiting after one cycle. This is the first step toward continuous operation.

```bash
# Agent stays alive, periodically checkpoints work as drafts
ta run "DevRel: ongoing issue triage and response drafting" \
  --source ./devrel-workspace --macro

# Agent emits checkpoints:
#   [checkpoint 1] 3 issue responses drafted → draft abc123
#   [checkpoint 2] blog post outlined → draft def456
#   [checkpoint 3] docs improvement PR → draft ghi789

# Human reviews each checkpoint independently
ta draft approve abc123   # issue responses look good
ta draft apply abc123 --git-commit

# Agent continues working, aware that checkpoint 1 was approved
```

**TA changes needed** (v0.4.1):
- `--macro` flag on `ta run` that keeps agent process alive
- Checkpoint protocol: agent signals "draft ready" without exiting
- `ta draft build --checkpoint` creates draft from current staging state
- Agent receives approval/rejection events via session channel
- Session persists across checkpoints (already built in v0.3.1.2)

---

### v0.4.4 — Interactive Session Completion

Wire up the session pause/resume mechanism (types exist, logic missing):

```bash
# Department agent is running in macro mode
# Human needs to give guidance mid-session
ta session show <id>                    # see current state
ta session send <id> "Focus on the security issue first"

# Agent receives message via SessionChannel, adjusts priorities
# Agent checkpoints a draft for the security response
ta draft view <id> --detail medium

# Human pauses the session (agent suspends)
ta session pause <id>

# Later, human resumes
ta session resume <id>
```

**TA changes needed** (v0.4.4):
- `ta session resume <id>` wiring (states exist, plumbing missing)
- `ta session send <id> "message"` for mid-session guidance
- Agent-side protocol for receiving `HumanInput::Message` during work

---

### v0.5.3 — Notification Channels

Remove the requirement that humans sit at a terminal:

```
Agent checkpoints draft ──→ TA fires on_draft_ready event
                                    │
                     ┌──────────────┼──────────────┐
                     ▼              ▼              ▼
               Slack card      Email summary    Discord embed
               [Approve]       Reply APPROVE    React ✅
               [Reject]        Reply REJECT     React ❌
               [View]          Link to web UI   /ta view
                     │              │              │
                     └──────────────┼──────────────┘
                                    ▼
                          TA processes approval
                          Agent continues or stops
```

**TA changes needed** (v0.5.3, already in PLAN.md):
- Notification connector trait (outbound: PR summary, inbound: approval)
- Email connector: SMTP send + IMAP reply parsing
- Slack connector: Block Kit cards + button callback handler
- Non-interactive approval API (token-based, for bot callbacks)

---

### v0.7 — Event System & JSON API (Phase 9 in VISION)

Make TA programmable so orchestrators can drive the loop:

```bash
# Orchestrator listens for TA events
ta events listen --json | while read event; do
  case $(echo $event | jq -r .type) in
    draft_ready)
      # Notify via Slack
      slack-post "#devrel" "$(echo $event | jq -r .summary)"
      ;;
    draft_approved)
      # Apply and start next cycle
      ta draft apply $(echo $event | jq -r .draft_id) --git-commit
      ta run "DevRel: next triage cycle" --source ./devrel-workspace --follow-up
      ;;
  esac
done
```

**TA changes needed** (new phase, maps to VISION Phase 9):
- `--json` output mode on all CLI commands
- `ta events listen` streaming endpoint
- Webhook callbacks on state transitions
- Stable event schema (JSON)

---

### v0.8+ — Virtual Office Runtime (Phase 11 in VISION)

The department becomes a first-class concept:

```yaml
# .ta/departments/devrel.yaml
department: devrel
description: "Developer Relations — community engagement and content"
remit: |
  Monitor GitHub issues, discussions, and social mentions.
  Draft responses, blog posts, and changelog entries.
  Suggest documentation improvements.
  Flag critical issues for human escalation.

schedule:
  - trigger: cron
    interval: "0 9 * * MON-FRI"     # weekday mornings
    goal: "DevRel: triage overnight issues and mentions"

  - trigger: webhook
    source: github
    event: issues.opened
    goal: "DevRel: assess and draft response to new issue"

  - trigger: event
    source: ta
    event: draft_applied
    goal: "DevRel: continue with next priority item"

agent: claude-code
alignment: devrel-profile            # from agents/devrel.yaml
capabilities:
  - "fs://workspace/docs/**"
  - "fs://workspace/blog/**"
  - "fs://workspace/responses/**"
  - "github://issues/read"
  - "github://discussions/read"
notification_channel: slack:#devrel-reviews

escalation:
  - condition: "risk_score > 7"
    action: "notify slack:#devrel-urgent"
  - condition: "no_approval_after: 24h"
    action: "notify email:lead@company.com"
```

```bash
ta department start devrel           # begins continuous operation
ta department status devrel          # show active goals, pending drafts
ta department pause devrel           # suspend all triggers
ta department stop devrel            # graceful shutdown
```

**TA changes needed** (new phase):
- Department definition schema (YAML)
- Trigger system: cron scheduler + webhook receiver + TA event listener
- `ta department start/stop/status/pause` commands
- Department-scoped alignment profiles (already designed in v0.4.0)
- Escalation rules engine
- Integration point for external orchestrators (Claude Flow, n8n)

---

## What a Department Session Looks Like Over a Week

```
Monday 9:00  │ [cron] DevRel wakes up
             │ ta run "Triage weekend issues" --source ./devrel --macro
             │ Agent reads 12 new issues, drafts 8 responses
             │ → Draft #1: 8 response files
             │ → Slack: "DevRel: 8 responses ready for review"
Monday 10:30 │ Human approves 6, discusses 2
             │ → Agent revises 2 discussed items
             │ → Draft #2: 2 revised responses
Monday 11:00 │ Human approves all → applied → committed
             │
Tuesday 9:00 │ [cron] DevRel wakes up
             │ Agent reads 3 new issues + 1 trending discussion
             │ → Draft #3: 3 responses + 1 blog post outline
             │ → Slack notification
Tuesday 14:00│ [webhook] Critical security issue opened
             │ ta run "Assess security issue #487" --source ./devrel
             │ Agent analyzes, flags risk_score=9
             │ → Escalation: notifies #devrel-urgent
             │ → Draft #4: security response + disclosure template
Tuesday 14:15│ Human approves security response immediately
             │
Wednesday    │ [cron] Normal triage cycle
             │ Agent notes blog post from Tuesday still in discuss
             │ → Draft #5: revised blog post + 2 new responses
             │ ...
```

---

## Mapping to PLAN.md Phases

The department agent pattern requires these capabilities, mapped to the earliest
phase where each could land:

| Capability | Earliest Phase | Required? |
|---|---|---|
| Follow-up goals with context injection | v0.3.0 (done) | Yes |
| Interactive sessions (types + storage) | v0.3.1.2 (done) | Yes |
| Alignment profiles per agent | v0.4.0 (done) | Yes |
| **Macro goals (multi-checkpoint)** | **v0.4.1** | **Critical** |
| **Session pause/resume wiring** | **v0.4.4** | **Critical** |
| Behavioral drift detection | v0.4.2 | Nice to have |
| Per-goal access constitutions | v0.4.3 | Nice to have |
| Credential broker | v0.5.0 | For external APIs |
| MCP tool interception | v0.5.1 | For external APIs |
| **Notification channels** | **v0.5.3** | **Critical** |
| JSON API on all commands | v0.7 (new) | For orchestration |
| Event streaming | v0.7 (new) | For orchestration |
| **Department runtime** | **v0.8+ (new)** | Full vision |

### Critical Path (minimum viable department)

```
v0.4.1 (Macro Goals)
  → Agent can checkpoint without exiting
  → Human can review checkpoints while agent continues

v0.4.4 (Session Completion)
  → Agent can be paused/resumed
  → Human can send mid-session guidance

v0.5.3 (Notifications)
  → Human doesn't need to sit at terminal
  → Approve from Slack/email/Discord
```

Everything else enhances the experience but isn't required for the core loop.

---

## Suggested PLAN.md Changes

### Accelerate

1. **v0.4.1 Macro Goals** — move from "planned" to "next". This is the single
   most important feature for continuous operation. Without it, every cycle
   requires a full goal teardown/setup.

2. **v0.4.4 Interactive Session Completion** — wire up the existing session
   types. The `InteractiveSession`, `SessionChannel`, and `HumanInput` types
   are already built. This is mostly plumbing.

### Add New Phase

3. **v0.7.x Event System & JSON API** — extract from VISION-virtual-office.md
   Phase 9 into a concrete PLAN.md phase. Scope: `--json` flag on all commands,
   `ta events listen`, webhook config in `workflow.toml`.

### Defer (Not Needed for MVP)

4. **v0.8+ Department Runtime** — the full `ta department start` experience is
   post-v1.0. The manual loop (v0.4.x) and notification-driven loop (v0.5.3)
   are sufficient for early adopters.

---

## Design Principle: TA Stays Thin

TA does NOT become the orchestrator. The department pattern works because:

- **TA** holds the checkpoint: "here's what the agent wants to do, approve it"
- **Orchestrator** (cron, Claude Flow, n8n, human) decides when to start the next cycle
- **Notification layer** lets humans approve from wherever they are

The department YAML (v0.8+) is syntactic sugar over: orchestrator config +
TA alignment profile + notification channel config. Each piece already has a
home in the architecture.
