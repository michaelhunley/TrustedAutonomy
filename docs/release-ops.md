# Release Operations Guide

Operational reference for the TA release pipeline. Covers certificate management,
platform signing, and recovery procedures.

---

## Windows Code Signing

### Why EV signing?

Windows SmartScreen blocks unsigned binaries with a "Windows protected your PC"
warning. There are two ways to clear it:

1. **Download reputation** — accumulates slowly over hundreds of installs. Even
   with a standard OV (Organization Validation) certificate, new releases start
   with zero reputation and trigger the warning.
2. **EV (Extended Validation) certificate** — Microsoft grants immediate reputation
   bypass. Users see the UAC prompt ("Do you want to allow **Trusted Autonomy** to
   make changes…") but no SmartScreen block, regardless of download count.

TA uses an EV certificate so every release — including the very first download — is
unblocked.

---

### Certificate procurement (one-time human step)

**Provider**: DigiCert, Sectigo, or GlobalSign (all Microsoft-trusted CAs). DigiCert
is recommended for its reliable TSA (`http://timestamp.digicert.com`) and fast
issuance.

**Type**: Extended Validation (EV) Code Signing Certificate

**Cost**: ~$300–500/yr (varies by provider and term)

**Lead time**: 1–5 business days. EV issuance requires identity verification of the
legal entity — have company incorporation documents ready.

**Publisher display name**: The Common Name (CN) in the certificate appears in Windows
UAC prompts as the publisher. Purchase with CN = `Trusted Autonomy` (or the exact
legal entity name) to display correctly.

**Format**: Export as PFX / PKCS#12 with a strong password.

---

### Storing the certificate as GitHub Actions secrets

Add two repository secrets under **Settings → Secrets and variables → Actions**:

| Secret name | Value |
|-------------|-------|
| `WINDOWS_SIGNING_CERT_BASE64` | Base64-encoded PFX: `base64 -w0 certificate.pfx` |
| `WINDOWS_SIGNING_PASSWORD` | PFX password |

To encode on macOS/Linux:
```bash
base64 -i certificate.pfx | tr -d '\n' | pbcopy   # macOS — copies to clipboard
base64 -w0 certificate.pfx                          # Linux — prints single line
```

On Windows (PowerShell):
```powershell
[Convert]::ToBase64String([IO.File]::ReadAllBytes("certificate.pfx")) | Set-Clipboard
```

---

### How signing works in CI

The `Sign Windows artifacts` step in `.github/workflows/release.yml` runs after the
WiX MSI build on the `x86_64-pc-windows-msvc` runner. It calls
`scripts/sign-windows.ps1` which:

1. Decodes `WINDOWS_SIGNING_CERT_BASE64` to a temp PFX file.
2. Signs `ta.exe`, `ta-daemon.exe`, and the `.msi` with `signtool.exe`:
   - Digest: SHA-256 (`/fd sha256`)
   - Timestamp: RFC 3161 via DigiCert TSA (`/tr http://timestamp.digicert.com /td sha256`)
3. Verifies each signature with `signtool verify /pa`.
4. Deletes the temp PFX (always, even on error).

The step is **skipped** when `WINDOWS_SIGNING_CERT_BASE64` is absent — forks and
PRs without the secret still build successfully.

---

### Verifying a signed MSI locally

Download the `.msi` from the GitHub release, then in PowerShell:

```powershell
# Requires Windows SDK (signtool.exe). Ships with Visual Studio or standalone SDK.
signtool verify /pa /v ta-<version>-x86_64-pc-windows-msvc.msi
```

Expected output ends with:
```
Number of files successfully Verified: 1
Number of warnings: 0
Number of errors: 0
```

Alternatively, right-click the MSI → **Properties → Digital Signatures** tab to
inspect the signature in the Windows UI.

---

### Certificate renewal

EV certs are typically valid for 1–3 years. Renew before expiry to avoid a gap:

1. Purchase the renewal from the same CA (or switch provider — any Microsoft-trusted
   CA works).
2. Export the new certificate as PFX.
3. Base64-encode the PFX (see above).
4. Update the `WINDOWS_SIGNING_CERT_BASE64` and `WINDOWS_SIGNING_PASSWORD` secrets
   in GitHub Actions.
5. Trigger a test release (e.g., `workflow_dispatch` on `release.yml`) to confirm
   the new cert signs and verifies correctly before the next production release.

**Tip**: Set a calendar reminder 60 days before the cert's `Not After` date. The
`signtool verify` output includes the expiry date — check it after each release.

---

### What to do if the cert expires mid-release

If a release is already in progress when you discover the cert is expired:

1. **Stop the release** — do not publish an unsigned build.
2. Purchase a new cert (DigiCert can expedite EV issuance in ~24h with pre-verified
   orgs).
3. Update the GitHub secrets.
4. Re-run the release workflow via `workflow_dispatch` with the same tag.

If assets were already uploaded to a draft release, check their status:
```bash
gh release view <tag> --json draft,assets
```
If all assets are present and `draft: true`, re-sign locally and re-upload, then
publish with:
```bash
gh release edit <tag> --draft=false
```

---

### Timestamp server

The signing script uses DigiCert's RFC 3161 TSA: `http://timestamp.digicert.com`

**Why this matters**: Authenticode signatures include a countersignature from the TSA
that records the signing time. After the certificate expires, Windows trusts the
signature if it was timestamped while the cert was valid. Without a timestamp,
signatures become invalid the moment the cert expires — previously-shipped installers
would then trigger SmartScreen warnings again.

---

## macOS Gatekeeper hardening

> **Status**: Planned — requires Apple Developer Program membership ($99/yr) and an
> App-Specific Password for `notarytool`. See PLAN.md phase v0.15.16 item 8.

Once the Apple Developer cert is available, the macOS DMG signing step will use:

```bash
codesign --deep --force --verify --sign "Developer ID Application: Trusted Autonomy" ta.pkg
xcrun notarytool submit ta.pkg --apple-id <email> --team-id <TEAM_ID> --password <app-specific-pwd> --wait
xcrun stapler staple ta.pkg
```

This eliminates the equivalent "macOS cannot verify the developer of this app" quarantine
warning. Until then, users can bypass it with **Control+click → Open**.
