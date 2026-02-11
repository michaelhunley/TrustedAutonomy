# Trusted Autonomy — Runner Specification

This document defines the **Runner**, the component responsible for executing arbitrary agent frameworks
inside a constrained sandbox while ensuring **all secured access flows through the Trusted Autonomy Gateway**.

The Runner is intentionally boring glue code. Its job is to make bypassing the control plane impossible,
while keeping agent frameworks completely unchanged.

---

## 1. Responsibilities (Non-Negotiable)

The Runner MUST:

1. Launch agent frameworks inside an isolated runtime.
2. Restrict filesystem access to the workspace mount only.
3. Prevent access to host credentials, secrets, and user home directories.
4. Control or block outbound network traffic.
5. Ensure all real-world access occurs via the Trust Autonomy Gateway.
6. Be cross-platform with one-click install semantics.
7. Be deterministic and auditable.

The Runner MUST NOT:
- implement business logic
- make policy decisions
- store credentials
- transform agent behavior

---

## 2. Execution Modes

### 2.1 `dev` (OSS default)
**Purpose:** Fast iteration, low friction.

- Agent runs as a normal local process.
- Workspace is a normal directory.
- Network access allowed, but credentials are absent.
- All writes and external effects are still staged by the gateway.

**Guarantee:** Prevents accidental side effects, not malicious bypass.

---

### 2.2 `research`
**Purpose:** Internet access without exfiltration.

- Same as `dev`, plus:
- Web access only via:
  - Gateway web tools, or
  - Forced HTTP(S) proxy controlled by the gateway.
- Retrieved content is sanitized and provenance-labeled.

**Guarantee:** Prevents prompt injection and uncontrolled writes.

---

### 2.3 `secure-local` (recommended)
**Purpose:** Prevent bypass by arbitrary frameworks.

- Agent runs in OCI container.
- gVisor (`runsc`) used as runtime.
- Filesystem:
  - Workspace mounted read/write.
  - Everything else read-only or absent.
- Network:
  - Default deny.
  - Allow only:
    - Gateway endpoint
    - Optional proxy endpoint.
- No credentials injected into container.

**Guarantee:** Agent cannot bypass the gateway.

---

### 2.4 `enterprise` (paid)
**Purpose:** Strong isolation and compliance.

- Same as `secure-local`, except:
- Runtime is Kata Containers (VM-level isolation).
- Centralized logging, audit retention, and SSO.

---

## 3. Platform Strategy

### Linux
- Native OCI execution.
- gVisor as default runtime.
- No VM required.

### macOS
- Lightweight Linux VM (Colima/Lima).
- Runner provisions VM automatically.
- Containers run inside VM using gVisor.

### Windows
- WSL2 auto-provisioned.
- Linux container runtime inside WSL2.
- gVisor used inside WSL2.

**Design principle:** End users never manually configure VMs.

---

## 4. Filesystem Model

- Workspace is mounted at `/workspace`.
- No access to:
  - `/home`
  - `/etc`
  - `/proc` (restricted)
  - host sockets or devices
- Optional read-only mount:
  - `/tools` for bundled utilities (rg, jq, formatters)

All file mutations are captured as ChangeSets by the gateway.

---

## 5. Network Model

### Default
- All outbound traffic blocked.

### Research Mode
One of:
1. Gateway-mediated web tools (preferred).
2. Forced HTTP proxy:
   - Proxy address injected as `HTTP_PROXY`.
   - Proxy enforces domain allowlists, logging, and quotas.

No raw sockets to the internet.

---

## 6. Credentials Model

- Zero credentials in the sandbox.
- No env vars with tokens.
- No mounted secret files.
- Gateway owns all OAuth/API credentials.

---

## 7. Launch Contract (Pseudo-JSON)

```json
{
  "goal_run_id": "uuid",
  "mode": "secure-local",
  "workspace_path": "/path/to/workspace",
  "agent_command": ["python", "agent.py"],
  "network_policy": {
    "allow_gateway": true,
    "allow_proxy": false
  },
  "resource_limits": {
    "cpu": "2",
    "memory": "4Gi"
  }
}
```

---

## 8. Failure Semantics

- If the Runner fails to start securely → abort Goal Run.
- If the sandbox attempts forbidden access → kill process and emit audit event.
- No silent fallback to insecure modes.

---

## 9. Why This Design Works

- The Runner removes ambient authority.
- The Gateway becomes the only door to the world.
- Agent frameworks remain unchanged.
- Security guarantees scale from OSS → Enterprise without rewrites.

---

## 10. Implementation Notes (Rust)

- Use `containerd` / `docker` APIs where possible.
- Runtime selection abstracted (`runc`, `runsc`, `kata`).
- Platform-specific provisioning hidden behind a single CLI command.
