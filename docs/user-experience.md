# Trusted Autonomy v1.0 — User Experience Walkthrough

> Based on PLAN.md phases v0.1–v1.0. Written to stress-test the product plan from five real user perspectives. Identifies gaps, over-engineering, and workflow rigidity.
>
> See also: `docs/enterprise-state-intercept.md` for the deep containment model (network-level capture for enterprise/regulated environments).

---

## How TA Changes the Supervision Model

Before diving into user types, the key shift:

| Mode | Standard Claude/Codex | TA-mediated |
|------|----------------------|-------------|
| **Active coding** | Continuous back-and-forth. ~100% attention. | `ta run` launches agent. Check back when draft ready. ~10-20% attention. |
| **Overnight/batch** | Not possible — session closes. | Agent works in background. Review next morning. 0% during execution. |
| **Auto-approved** | N/A | Supervisor reviews within constitutional bounds. User sees summary. ~1% attention. |
| **Virtual office** | N/A | Roles run on triggers. Review when notified. Minutes per day. |

**The shift**: Standard agents demand synchronous attention. TA shifts to asynchronous review. Trust escalates over time — the more the supervisor proves reliable, the less the human needs to intervene.

---

## 1. Software Engineer

**Who**: Full-stack dev, uses Claude Code / Codex / Cursor daily. Has a Rust+React monorepo. Wants AI to handle the tedious stuff safely.

### Day-to-day workflow at v1.0

```
# Morning: pick up where yesterday's agent left off
ta plan next                        # "v0.8.2 — API pagination. Suggest: ta run ..."
ta run "Add cursor pagination" \
  --source . --phase 0.8.2 --macro  # macro goal: agent stays in session

# Agent works in staging. I watch output stream in terminal (--interactive).
# Agent hits a design question — I see it in real-time, type guidance.
# Agent builds sub-goal drafts as it goes. Each one pauses for my review.

ta draft view <id>                  # same format I'll see in the git commit
# Looks good — approve inline from the session, or:
ta draft approve <id>

# Agent continues to next sub-goal. Repeat until done.
# Session ends. All approved changes applied. Git commit auto-created.

# Afternoon: overnight agent handled the refactor
ta draft list                       # 3 drafts auto-approved by supervisor
ta audit drift claude-code          # any unusual behavior? No drift detected.
# Supervisor auto-approved because: <10 files, risk score 8, all in src/
# I see the summary, trust the supervisor, move on.
```

### What they need to know

- `ta run` / `ta draft` / `ta plan` — three commands cover 90% of use
- `.ta/workflow.toml` for project config (git integration, enforcement level)
- Agent YAML configs exist but defaults work — auto-detection picks the right agent
- Review model: every change is a "draft" you approve/reject, like a PR but richer
- Zero-config path works: `ta run "fix the login bug" --source .` needs nothing pre-configured

### What problems TA solves

1. **AI changes without review**: 40-file change with structured diffs and per-file explanations, not a mystery commit
2. **Wasted tokens on re-learning**: Session resume preserves context via agent-native mechanisms
3. **Multi-agent chaos**: Alignment profiles prevent agents stepping on each other
4. **"What did the agent do?"**: Hash-verified audit trail with provenance
5. **Compliance as byproduct**: ISO/EU AI Act evidence generated automatically
6. **Credential safety**: Agents never see raw API keys or OAuth tokens — TA brokers all access

### Supervision frequency

- **Starting out**: Review every draft manually. ~15 min/day for a typical project.
- **After trust builds**: Set up constitutional auto-approval. Review only escalations and daily summary. ~5 min/day.
- **Mature usage**: Supervisor handles routine, human handles strategy. Drafts per week: 20+ agent-reviewed, 2-3 human-reviewed.

### Remaining friction

- Must learn TA vocabulary (goals, drafts, phases, dispositions) on top of git
- Staging copy doubles disk for large repos (COW planned for future)
- Ad-hoc `ta run "fix bug"` with no plan must feel as frictionless as `claude "fix bug"`

---

## 2. Product / Business Person

**Who**: Manages a team shipping a SaaS product. Not a coder. Uses email, Slack, Google Docs daily. Wants AI assistants to handle routine comms and reporting — but needs to trust what they send.

### Setup experience (v0.7.0 guided setup)

```
# Install TA (desktop installer bundles everything)
# First run launches setup assistant — itself a TA-mediated agent:

ta setup --template email-assistant

# Agent asks questions in plain language:
#   "Which email account should I manage? (I'll open a Google sign-in)"
#   "What kinds of emails should I draft replies for?"
#   "Where should I notify you when drafts are ready? (Slack/email/web)"
#
# Agent proposes a config. TA shows it as a draft:
#   - workflow.toml with email role definition
#   - Gmail OAuth credentials (stored in vault, never shown)
#   - Constitutional bounds: "auto-draft for routine, escalate for new contacts"
#
# Review and approve. Config activates.
```

### Day-to-day workflow at v1.0

```
# Monday morning: notification arrives in Slack
# "3 drafts ready: Weekly Status Report (2 emails + 1 Slack message)"
# Click through to web review UI (localhost or LAN)

# See exactly what the agent wants to do:
#   - Send email to team@company.com: "Q1 Progress Update" (preview full body)
#   - Post to #general Slack: summary message (preview)
#   - Update Google Doc: status tracker (see diff)

# Approve the email, edit the Slack message, reject the doc update with note.
# Agent sees rejection, revises, resubmits. Approve the revision.

# Customer reply came in overnight. Agent drafted a response.
# Review in Slack thread (reply "approve" or type feedback).
# Approved. Agent sends via TA credential broker — never touches the OAuth token.
```

### What they need to know

- How to review and approve/reject (web UI or Slack/email)
- How to give feedback when agent gets something wrong (comment on draft)
- Setup wizard handles everything else — no YAML, no CLI

### What problems TA solves

1. **Fear of AI sending bad emails**: Every outbound action held for review. See exactly what goes out.
2. **Black box automation**: See agent's reasoning alongside the output
3. **Trust escalation**: Start manual, gradually auto-approve low-risk actions
4. **Credential safety**: Agent never has your Gmail password or OAuth token. TA brokers all access.
5. **Consistency**: Agent constrained by constitution, not just prompted

### Supervision frequency

- **Week 1**: Review every draft. ~20 min/day. Learning what the agent does well.
- **Month 1**: Constitutional auto-approval for routine emails. Review only new contacts, complex threads. ~5 min/day.
- **Mature**: Daily summary notification. Handle 2-3 escalations per day. ~3 min/day.

### Remaining friction

- Web review UI must be polished enough for non-technical users (v0.5.2 is minimal)
- MCP server health/connectivity issues need clear error messages, not stack traces
- Multi-user (team) workflows not addressed until v1.0 virtual office

---

## 3. Home User (Email & Social Media)

**Who**: Non-technical. Uses email, Instagram, X/Twitter. Wants an AI assistant to manage social presence, respond to routine emails, schedule posts.

### Setup experience

```
# Install TA from desktop installer (Mac/Windows/Linux)
# Opens setup wizard in browser (localhost web UI)

# Conversational flow:
#   "What would you like help with?"
#   > "Managing my Instagram and handling routine emails"
#
#   "Let's connect your accounts." (OAuth popups for Gmail, Instagram)
#   "How often should I post to Instagram?"
#   > "3 times a week from my photo library"
#
#   "Here's what I've set up. Review before activating:"
#   [Shows proposed config as a simple checklist, not YAML]
#   ✅ Email: draft replies to routine emails, escalate business inquiries
#   ✅ Instagram: 3 posts/week, you review captions before posting
#   ✅ Notifications: email digest at 9am with pending drafts
#
#   [Approve] → Config activates.
```

### Day-to-day workflow at v1.0

```
# Morning: open TA app (web UI), see dashboard
#   - 2 Instagram posts drafted (preview images + captions)
#   - 4 email replies drafted
#   - 1 flagged: "Business inquiry — needs your input"

# Tap each draft. Swipe to approve/reject.
# Edit caption on Instagram post before approving.
# Type guidance for the flagged email: "Tell them I'm available next week"
# Agent revises and resubmits. Approve.

# Weekly: check summary
#   15 posts published, 28 emails handled
#   0 alerts, agent staying on track
#   Cost: $3.40 in API tokens this week
```

### What they need to know

- How to review and approve (tap/swipe in web UI)
- How to give feedback when the agent gets it wrong
- Where to adjust preferences ("don't post before 10am")

### What problems TA solves

1. **Fear of AI acting on their behalf**: Nothing happens without approval
2. **Social media burnout**: Agent handles routine, human handles creative
3. **Email overwhelm**: Agent triages and drafts, human handles judgment calls
4. **Privacy**: Local-first — data stays on their machine
5. **Cost visibility**: See exactly what the AI costs each week

### Supervision frequency

- **Week 1**: Review everything. ~15 min/day.
- **Month 1**: Auto-approve routine email replies (low risk, familiar contacts). ~5 min/day.
- **Mature**: Glance at daily digest. Handle 1-2 items. ~2 min/day.

### Remaining friction

- No mobile native app (PWA must work well on phone)
- Onboarding must be conversational, never show YAML or terminal
- Cost must be predictable — "this month will cost ~$15" not surprises

---

## 4. Home Finance Manager

**Who**: Manages household budget, tracks investments, pays bills. Wants dashboards and reports without spreadsheet drudgery. May also be a family office administrator managing multiple accounts.

### Setup experience

```
ta setup --template home-finance

# Agent guides through:
#   "Let's connect your bank accounts." (Plaid OAuth flow — TA stores credentials)
#   "Which accounts? Checking, savings, credit card, brokerage?"
#   "How should I categorize transactions? (I'll learn from your corrections)"
#   "Monthly report format: dashboard HTML, email summary, or both?"
#
# Proposed config as draft:
#   ✅ Plaid connection: Chase checking, Fidelity brokerage (read-only)
#   ✅ Weekly categorization review: agent categorizes, you review mistakes
#   ✅ Monthly dashboard: spending by category, budget vs actual, investment performance
#   ✅ Bill monitoring: flag anomalies (unexpected charges, price increases)
#   ✅ Constitutional bounds: read-only access — agent cannot initiate transfers
#
# [Approve] → Activates.
```

### Day-to-day workflow at v1.0

```
# Weekly: notification "Transaction review ready"
# Open web UI: 47 transactions this week, agent categorized all
#   - 3 flagged as uncertain: "Costco — Groceries or Household?"
#   - 1 anomaly: "Netflix increased from $15.49 to $22.99"
# Correct the 3 categories. Agent learns for next time.
# Acknowledge the Netflix increase.

# Monthly: "Monthly finance report ready"
# Open HTML dashboard (rendered by TA's output adapter system):
#   - Spending by category with month-over-month trends
#   - Budget vs actual with variance alerts
#   - Investment portfolio performance (daily/monthly/YTD)
#   - Upcoming bills with estimated dates
# Agent also drafted a summary email to spouse. Review and approve sending.

# Tax season: "Tax document prep ready"
# Agent collected all deductible transactions, organized by category
# Generated summary PDF for accountant. Review draft before sharing.
```

### Family office extension

```
# Setup adds multi-account, multi-person structure:
ta setup --template family-office

# Tiered access (enforced by TA credential broker):
#   Principal: sees all accounts, all reports, can approve transfers
#   Advisor: sees portfolio data, generates reports, no transaction detail
#   Accountant: sees tax-relevant transactions only, during tax season only
#
# Each person authenticates via SSO/OAuth — gets their scoped view
# Agent runs reports for each tier separately — same data, different views
# All access logged in audit trail
```

### What problems TA solves

1. **Spreadsheet drudgery**: Agent categorizes, tracks, reports. Human reviews and decides.
2. **Financial data security**: Local-first, credentials in encrypted vault, agents have read-only access
3. **Audit trail**: Every agent access to financial data is logged — who, what, when
4. **Family office complexity**: Multi-account, tiered access without building a custom app
5. **Tax prep**: Agent does the collection and organization; human reviews before sharing with accountant

### Supervision frequency

- **Weekly**: 5-10 min reviewing categorizations and flagged items
- **Monthly**: 10 min reviewing dashboard and report before sharing
- **Tax season**: 30 min reviewing collected documents before sending to accountant

---

## 5. Areas to Examine Before Going Too Far

### Critical path validation (do these spikes before committing)

1. **MCP interception spike**: Intercept one MCP tool call, hold it, replay it. Confirm the pattern works. This is the biggest architecture bet — if it doesn't work cleanly, v0.5+ needs redesign.
2. **Credential broker spike**: OAuth flow → encrypted storage → scoped session token → MCP server call. End-to-end. If this is clunky, every non-filesystem use case suffers.
3. **Web UI spike**: Serve one HTML page from `ta daemon`, render one draft, approve it. If the daemon architecture doesn't support this cleanly, v0.5.2 is harder than expected.
4. **Plaid integration spike**: Connect one bank account, fetch transactions, display in `ta draft view`. Validates the finance use case isn't blocked by API complexity.
5. **Auto-approval spike**: Supervisor agent evaluates one draft against a constitutional config. If the LLM-based verification is unreliable, v0.6.0 needs a different approach (rule-based only).

### Workflow openness checklist

| Area | Open or fixed? | Status |
|------|---------------|--------|
| Agent framework | Open — any CLI agent (Claude, Codex, custom) via YAML config | Good |
| External services | Open — any MCP server, no built-in service clients | Good |
| Plan format | Open after v0.3.1.1 — schema-driven parsing | Good |
| Review channel | Open after v0.3.1.2 — SessionChannel trait, any adapter | Good |
| Auth/identity | Open after v0.5.0 — OAuth, API key, SSO, custom | Good |
| Output format | Open — terminal, HTML, JSON, markdown adapters | Good |
| Workflow logic | **Risk**: Constitutional configs (v0.6.0) could become rigid. Ensure they're composable, not monolithic. | Watch |
| Trigger system | **Risk**: v1.0 trigger system (cron, webhook, event) may not cover all cases. Ensure extensible. | Watch |
| Finance integrations | **Risk**: Plaid is US-centric. International users need Open Banking, Yodlee, or manual CSV import. | Watch |

### What NOT to build (leave to the ecosystem)

- Custom MCP servers for specific services — use existing ones, contribute patches upstream
- A mobile native app — PWA is sufficient for v1.0
- An LLM model — use Claude/GPT/local models via agent framework
- A cloud hosting platform — let users self-host; cloud can be a wrapper later
- Accounting software — TA generates reports, doesn't replace QuickBooks

### Suggested priority order

The current plan phases are mostly in the right order. Key reorderings already made:
1. **Credential broker** moved to v0.5.0 (prerequisite for all external actions)
2. **Web review UI** moved to v0.5.2 (unblocks non-dev users early)
3. **Notifications** moved to v0.5.3 (needed immediately when external actions land)
4. **Supervisor/auto-approval** becomes v0.6 (unlocks async supervision model)
5. **Network intercept** extracted to enterprise doc (not blocking v1.0)
6. **Sandbox** becomes optional hardening at v0.9.1 (not a prerequisite)
7. **Setup wizard + templates** becomes v0.7 (enables non-dev onboarding)
