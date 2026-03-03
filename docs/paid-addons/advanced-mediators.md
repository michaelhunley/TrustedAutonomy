# Advanced Mediators (Paid Add-On)

> **Layer**: L1 — Resource Mediation
> **Status**: Planned
> **Boundary**: TA core includes `FsMediator` (file staging). Advanced mediators add production-grade staging for databases, cloud APIs, and external services.

---

## What It Does

Production-quality `ResourceMediator` implementations for non-file resources. Each mediator stages mutations, generates human-readable previews, applies after approval, and supports rollback where possible.

## Mediators

### Database Mediator (`db://`)
- **Providers**: Postgres, MySQL, SQLite, DynamoDB

**Session-local staging database.** The core design challenge: when an agent issues a `SELECT` after an `INSERT`, it must see its own writes — but nothing touches the real database until the human approves. The DB mediator solves this with a **staging overlay**:

1. **On session start**: Create a session-local staging layer.
   - **Postgres/MySQL**: Open a long-lived transaction with `BEGIN` + `SAVEPOINT` per mutation. Reads within the session go through this transaction and see uncommitted writes.
   - **SQLite**: Clone the database file (or use an in-memory overlay via `ATTACH`). All session I/O targets the clone.
   - **DynamoDB**: Maintain an in-memory write cache keyed by partition+sort key. Reads merge the cache with the real table (read-your-writes).

2. **On mutation (INSERT/UPDATE/DELETE)**: Record the SQL in the staged mutation log *and* execute it against the staging layer so subsequent reads see the change.

3. **On read (SELECT)**: Route through the staging layer. The agent sees a consistent view of its own changes without the real database being modified.

4. **Preview**: Show the SQL statement, affected table/row count estimate, schema impact analysis. For multi-statement sessions, show the full changeset as a diff (rows added/modified/deleted).

5. **Apply** (after approval): Replay the staged mutations against the real database within a transaction. Commit on success.

6. **Rollback**: Discard the staging layer. For Postgres/MySQL, `ROLLBACK` the transaction. For SQLite, delete the clone. Nothing was written to the real database.

7. **Safety**: Read-only queries pass through without staging. `DROP`, `TRUNCATE`, `ALTER` always require approval regardless of policy.

**Trade-offs**: The staging overlay adds latency for reads (merge step) and memory for the write cache. For large data volumes, the staging layer may need to spill to disk. Long-running sessions risk conflicts if the underlying data changes — on apply, the mediator detects conflicts and reports them for human resolution.

### Cloud API Mediator (`cloud://`)
- **Providers**: AWS (via boto3/SDK), GCP, Azure
- **Staging**: Serialize the API call (service, action, parameters) as a `StagedMutation`. For IaC, show the Terraform/Pulumi plan diff.
- **Preview**: Human-readable summary of the cloud action ("Create an S3 bucket named X with public access disabled").
- **Apply**: Execute the API call. Record the response for audit.
- **Rollback**: Best-effort reverse (delete created resources, restore modified configs). Some actions are not reversible — flag these in preview.
- **Cost estimation**: Estimate the cost impact of the proposed change before approval.

### Social Media Mediator (`social://`)
- **Providers**: Twitter/X, LinkedIn, Bluesky, Mastodon
- **Staging**: Create a draft post via provider API (or store locally if no draft API).
- **Preview**: Rendered post preview with character count, media attachments, scheduling info.
- **Apply**: Publish the draft.
- **Rollback**: Delete the published post (if within provider's deletion window).

## Integration with TA Core

Each mediator implements:
```rust
trait ResourceMediator: Send + Sync {
    fn scheme(&self) -> &str;
    fn stage(&self, action: ProposedAction) -> Result<StagedMutation>;
    fn preview(&self, staged: &StagedMutation) -> Result<MutationPreview>;
    fn apply(&self, staged: &StagedMutation) -> Result<ApplyResult>;
    fn rollback(&self, staged: &StagedMutation) -> Result<()>;
    fn classify(&self, action: &ProposedAction) -> ActionClassification;
}
```

Registered via `MediatorRegistry` in the MCP gateway. Configuration in `.ta/config.yaml`:
```yaml
mediators:
  db:
    enabled: true
    provider: postgres
    credential: "pg-prod"
  cloud:
    enabled: true
    provider: aws
    credential: "aws-deploy"
```

## Delivery Model

- Separate crates: `ta-mediator-db`, `ta-mediator-cloud`, `ta-mediator-social`
- Each crate handles multiple providers via feature flags or runtime config
- Provider-specific dependencies are isolated (no Postgres deps if you only use AWS)
