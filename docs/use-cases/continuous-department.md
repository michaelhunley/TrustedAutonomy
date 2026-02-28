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

## The ReviewChannel Architecture (v0.4.1.1)

The key architectural insight: **every TA interaction point is a message through
a pluggable channel**. Draft review, approval, discussion, plan negotiation,
escalation — all flow through the same `ReviewChannel` trait.

```
Agent calls MCP tool (ta_draft submit, ta_plan update, etc.)
       │
       ▼
MCP Gateway routes to TA core logic
       │
       ▼
TA emits InteractionRequest {
  kind: DraftReview | ApprovalDiscussion | PlanNegotiation | Escalation
  urgency: Blocking | Advisory | Informational
  context: { draft_id, goal_id, summary, ... }
}
       │
       ▼
ReviewChannel delivers to human via configured medium
       │
       ├── TerminalChannel (v0.4.1.1) → stdin/stdout
       ├── SlackChannel (future) → Block Kit card + button callback
       ├── DiscordChannel (future) → embed + reaction handler
       ├── EmailChannel (future) → SMTP send + IMAP reply parse
       └── WebhookChannel (future) → POST to URL, await callback
       │
       ▼
Human responds through same channel
       │
       ▼
InteractionResponse unblocks the MCP handler
       │
       ▼
Agent receives result, continues working
```

This means:
- `ta draft approve` is sugar over `InteractionResponse { approve: true }` via CLI
- A Slack "Approve" button sends the same response via `SlackChannel`
- The agent doesn't know or care which channel the human used
- Adding a new channel (SMS, mobile app) requires only implementing the trait

---

## How It Works with TA (Phase by Phase)

### Today (v0.4.1) — Manual Loop with Macro Goals

Macro goals (v0.4.1) added the data model: `is_macro`, `parent_macro_id`,
`sub_goal_ids`, MCP tools (`ta_draft`, `ta_goal_inner`, `ta_plan`), and
`PrReady → Running` state transition for inner-loop iteration.

The agent can now decompose work, submit drafts, and continue — but the
runtime loop and human communication channel aren't wired yet.

```bash
# Start a macro goal session
ta run "DevRel: triage this week's GitHub issues" \
  --source ./devrel-workspace --macro

# Agent works in staging, uses MCP tools to:
#   ta_draft build → create draft from current changes
#   ta_draft submit → signal "ready for review"
#   ta_goal_inner start → create sub-goals for different work items

# When agent exits, review accumulated drafts
ta draft list                          # see all drafts with age
ta draft approve <id>                  # approve first
ta draft apply <id> --git-commit       # then apply

# Follow-up for discussed items
ta run "DevRel: revise blog draft" --source ./devrel-workspace --follow-up
```

**What works**: Macro goal hierarchy, sub-goals, MCP tools, follow-up context.
**What's manual**: Human must poll `ta draft list`, approve then apply separately.

### v0.4.1.1 — Runtime Loop + TerminalChannel (Next)

This is the critical phase. It connects the v0.4.1 data model to a live
human-agent communication loop:

1. `ta run --macro` starts the MCP gateway alongside the agent
2. Agent calls `ta_draft submit` → MCP handler emits `InteractionRequest`
3. `TerminalChannel` renders the request in the terminal
4. Human types response (approve/reject/discuss)
5. Response flows back to the MCP handler, unblocks the agent
6. Agent continues working with the feedback

```bash
# Single command — agent stays alive, human interacts inline
ta run "DevRel: ongoing triage" --source ./devrel-workspace --macro

# Terminal shows:
#   [Agent] Built draft abc123: 8 issue responses
#   [TA] Draft ready for review. Approve? [a]pprove / [r]eject / [d]iscuss
#   > a
#   [TA] Approved. Agent continuing...
#   [Agent] Built draft def456: blog post outline
#   [TA] Draft ready for review. Approve? [a]pprove / [r]eject / [d]iscuss
#   > d need more detail on the performance section
#   [TA] Discussion noted. Agent revising...
```

**The department works here** — with one human at a terminal.

### v0.5.3+ — Additional ReviewChannel Adapters

Adding SlackChannel, DiscordChannel, EmailChannel is now just implementing
the `ReviewChannel` trait. The core logic doesn't change — only the delivery
and response collection mechanism.

```yaml
# .ta/config.yaml
review:
  channel: slack
  slack:
    channel_id: C12345
    bot_token_env: SLACK_BOT_TOKEN
```

```
Agent submits draft ──→ TA emits InteractionRequest
                              │
                              ▼
                     SlackChannel delivers:
                     ┌──────────────────────────────┐
                     │ TA Draft Ready                │
                     │ "8 issue responses drafted"   │
                     │ [Approve] [Reject] [Discuss]  │
                     └──────────────────────────────┘
                              │
                     Human taps [Approve]
                              │
                              ▼
                     InteractionResponse → agent continues
```

**The department no longer needs a terminal.** Human approves from phone.

### v0.8+ — Department Runtime (Full Vision)

Department YAML becomes syntactic sugar over: orchestrator config + alignment
profile + ReviewChannel config + trigger system.

```yaml
# .ta/departments/devrel.yaml
department: devrel
remit: "Community engagement and content"
agent: claude-code
alignment: devrel-profile
channel: slack
schedule:
  - trigger: cron
    interval: "0 9 * * MON-FRI"
    goal: "Triage overnight issues"
  - trigger: webhook
    source: github
    event: issues.opened
    goal: "Assess and draft response"
escalation:
  - condition: "risk_score > 7"
    action: "notify slack:#devrel-urgent"
```

---

## What a Department Session Looks Like Over a Week

```
Monday 9:00  │ [cron] DevRel wakes up
             │ ta run "Triage weekend issues" --source ./devrel --macro
             │ Agent reads 12 new issues, drafts 8 responses
             │ → InteractionRequest(DraftReview) → SlackChannel
             │ → Slack: "DevRel: 8 responses ready for review"
Monday 10:30 │ Human taps Approve on 6, Discuss on 2 via Slack
             │ → InteractionResponse flows back to agent
             │ → Agent revises 2 discussed items
             │ → New InteractionRequest(DraftReview) for revisions
Monday 11:00 │ Human approves all via Slack → applied → committed
             │
Tuesday 9:00 │ [cron] DevRel wakes up, same flow
             │
Tuesday 14:00│ [webhook] Critical security issue opened
             │ Agent flags risk_score=9
             │ → InteractionRequest(Escalation, urgency: Blocking)
             │ → Escalation routes to #devrel-urgent (different channel)
             │ → Human approves security response immediately
             │
Wednesday    │ [cron] Normal triage cycle continues
```

---

## Mapping to PLAN.md Phases

| Capability | Phase | Status |
|---|---|---|
| Follow-up goals with context injection | v0.3.0 | Done |
| Interactive sessions (types + storage) | v0.3.1.2 | Done |
| Alignment profiles per agent | v0.4.0 | Done |
| Macro goals (data model + MCP tools) | v0.4.1 | Done |
| **ReviewChannel + TerminalChannel + runtime loop** | **v0.4.1.1** | **Next** |
| Behavioral drift detection | v0.4.2 | Pending |
| Per-goal access constitutions | v0.4.3 | Pending |
| **Slack/Discord/Email channel adapters** | **v0.5.3** | Pending |
| JSON API on all commands | v0.7 (planned) | Pending |
| Department runtime | v0.8+ (vision) | Future |

### Critical Path (minimum viable department)

```
v0.4.1.1 (ReviewChannel + TerminalChannel + runtime loop)
  → Agent and human communicate inline during macro session
  → Every interaction point uses the same protocol
  → Department works at a terminal

v0.5.3 (Additional channel adapters)
  → Human approves from Slack/email/Discord
  → Department works without a terminal
```

v0.4.4 (Interactive Session Completion) is **absorbed by v0.4.1.1** — the
ReviewChannel with TerminalChannel IS the session completion mechanism.
Pause/resume is "channel goes quiet / channel reconnects."

---

## Design Principle: TA Stays Thin

TA does NOT become the orchestrator. The department pattern works because:

- **TA** holds the checkpoint: "here's what the agent wants to do, approve it"
- **ReviewChannel** is the interaction protocol, not a notification layer
- **Orchestrator** (cron, Claude Flow, n8n, human) decides when to start cycles
- **Channel adapters** let humans respond from wherever they are

The ReviewChannel trait is the spine that everything hangs on. Adding a new
interaction medium (SMS, mobile app, web UI) requires only implementing the
trait — no changes to TA core, the agent, or the MCP gateway.
