# Enterprise Channels (Paid Add-On)

> **Layer**: L5 — IO & Delivery
> **Status**: Planned
> **Boundary**: TA core includes TerminalChannel and WebhookChannel. Enterprise channels add production-grade integrations for team environments.

---

## What It Does

Pluggable `ReviewChannel` and `SessionChannel` implementations for enterprise communication platforms. Each channel is a separate crate implementing TA's existing traits.

## Channels

### Microsoft Teams
- Review notifications with approval buttons (Adaptive Cards)
- Thread-based draft review discussions
- Session streaming in a dedicated channel
- OAuth2 integration via TA credential vault

### ServiceNow
- Draft reviews as ServiceNow tickets (change requests)
- Approval workflow integrated with ServiceNow's approval engine
- Audit trail synced to ServiceNow's CMDB
- ITSM-compliant change management

### PagerDuty
- Escalation notifications when policy violations or drift detected
- On-call routing for urgent approval requests
- Incident creation for failed or denied drafts
- Integration with PagerDuty's escalation policies

### Jira
- Draft reviews as Jira issues
- Link TA goals to Jira epics/stories
- Status sync between TA goal states and Jira workflow

## Integration with TA Core

Each channel implements:
```rust
trait ChannelFactory: Send + Sync {
    fn build_review(&self, config: &Value) -> Result<Box<dyn ReviewChannel>>;
    fn build_session(&self, config: &Value) -> Result<Box<dyn SessionChannel>>;
    fn capabilities(&self) -> ChannelCapabilities;
}
```

Registered via `ChannelRegistry` at startup. Configuration in `.ta/config.yaml`:
```yaml
channels:
  review:
    type: teams              # or: servicenow, pagerduty, jira
    credential: "teams-oauth"
    channel: "Engineering"
```

## Delivery Model

- Separate crates: `ta-channel-teams`, `ta-channel-servicenow`, `ta-channel-pagerduty`, `ta-channel-jira`
- Compiled into the TA daemon as optional features, or loaded as plugins
- Each crate has its own integration tests against provider sandboxes
