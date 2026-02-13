# Homebrew Formula for Trusted Autonomy

This document describes how to create and maintain a Homebrew formula for TA, enabling macOS users to install with `brew install trustedautonomy/tap/ta`.

## Why Homebrew?

Homebrew is the most popular package manager for macOS:
- Familiar to developers (`brew install <package>`)
- Handles dependencies and PATH setup automatically
- Supports versioning and upgrades (`brew upgrade ta`)
- Better than manual install for many users

## Prerequisites

- GitHub repository for the tap (e.g., `trustedautonomy/homebrew-tap`)
- Released binaries on GitHub Releases (✓ already have this)
- SHA256 checksums for binaries (✓ release workflow generates these)

## Homebrew Terminology

- **Formula**: Ruby file defining how to install a package
- **Tap**: Third-party repository of formulas (like a PPA for apt)
- **Bottle**: Pre-compiled binary (faster than building from source)
- **Cellar**: Where Homebrew installs packages (`/usr/local/Cellar` or `/opt/homebrew/Cellar`)

## Setup: Create a Custom Tap

### Step 1: Create the Tap Repository

```bash
# Create a new GitHub repository named: homebrew-tap
# (Homebrew requires the "homebrew-" prefix)
gh repo create trustedautonomy/homebrew-tap --public --description "Homebrew formulae for Trusted Autonomy"

# Clone it locally
git clone https://github.com/trustedautonomy/homebrew-tap.git
cd homebrew-tap

# Create Formula directory
mkdir -p Formula
```

### Step 2: Create the Formula

Create `Formula/ta.rb`:

```ruby
class Ta < Formula
  desc "Trusted Autonomy — local-first agent substrate"
  homepage "https://github.com/trustedautonomy/ta"
  version "0.1.0-alpha"
  license "Apache-2.0"

  # macOS Apple Silicon (arm64)
  if Hardware::CPU.arm?
    url "https://github.com/trustedautonomy/ta/releases/download/v0.1.0-alpha/ta-v0.1.0-alpha-aarch64-apple-darwin.tar.gz"
    sha256 "REPLACE_WITH_ACTUAL_SHA256_FOR_ARM64"
  else
    # macOS Intel (x86_64)
    url "https://github.com/trustedautonomy/ta/releases/download/v0.1.0-alpha/ta-v0.1.0-alpha-x86_64-apple-darwin.tar.gz"
    sha256 "REPLACE_WITH_ACTUAL_SHA256_FOR_X86_64"
  end

  def install
    # Install the binary
    bin.install "ta"
  end

  test do
    # Test that the binary runs and shows version
    assert_match version.to_s, shell_output("#{bin}/ta --version")
  end
end
```

### Step 3: Get SHA256 Checksums

After creating a release, download the SHA256 files:

```bash
# For arm64 (Apple Silicon)
curl -fsSL https://github.com/trustedautonomy/ta/releases/download/v0.1.0-alpha/ta-v0.1.0-alpha-aarch64-apple-darwin.tar.gz.sha256

# For x86_64 (Intel)
curl -fsSL https://github.com/trustedautonomy/ta/releases/download/v0.1.0-alpha/ta-v0.1.0-alpha-x86_64-apple-darwin.tar.gz.sha256
```

Update the `sha256` values in the formula with the actual checksums.

### Step 4: Test the Formula Locally

```bash
# Test installation from local tap
brew install --build-from-source ./Formula/ta.rb

# Test that it works
ta --version

# Uninstall for testing
brew uninstall ta
```

### Step 5: Publish the Tap

```bash
git add Formula/ta.rb
git commit -m "Add ta formula v0.1.0-alpha"
git push origin main
```

## Using the Tap

Once published, users can install TA via Homebrew:

```bash
# Add the tap (one-time setup)
brew tap trustedautonomy/tap

# Install ta
brew install ta

# Later: upgrade to new version
brew upgrade ta

# Uninstall
brew uninstall ta
```

Or install in one command:

```bash
# Brew can auto-tap when using full formula path
brew install trustedautonomy/tap/ta
```

## Updating the Formula for New Releases

When releasing a new version (e.g., v0.1.0-beta):

### Automated: Use brew bump-formula-pr

```bash
# Homebrew provides a tool to automate formula updates
brew bump-formula-pr \
  --url="https://github.com/trustedautonomy/ta/releases/download/v0.1.0-beta/ta-v0.1.0-beta-aarch64-apple-darwin.tar.gz" \
  --sha256="NEW_ARM64_SHA256" \
  trustedautonomy/tap/ta
```

This creates a PR in your tap repository with the version bump.

### Manual: Edit the Formula

1. Update the `version` line
2. Update the `url` lines to point to new release
3. Update the `sha256` values
4. Commit and push

```bash
cd homebrew-tap

# Edit Formula/ta.rb
vim Formula/ta.rb

# Test locally
brew reinstall ta

# Push update
git add Formula/ta.rb
git commit -m "Bump ta to v0.1.0-beta"
git push origin main
```

### Automated via GitHub Actions (Future)

Consider automating formula updates when a new release is created:

```yaml
# .github/workflows/update-homebrew.yml (in main TA repo)
name: Update Homebrew Formula

on:
  release:
    types: [published]

jobs:
  update-formula:
    runs-on: ubuntu-latest
    steps:
      - name: Update Homebrew formula
        uses: dawidd6/action-homebrew-bump-formula@v3
        with:
          token: ${{ secrets.HOMEBREW_TAP_TOKEN }}
          formula: ta
          tap: trustedautonomy/homebrew-tap
          tag: ${{ github.ref_name }}
```

## Advanced: Bottles (Pre-compiled Binaries)

Homebrew "bottles" are pre-compiled binaries that install faster than building from source.

Since TA releases already include pre-built binaries, you can specify them as bottles:

```ruby
class Ta < Formula
  desc "Trusted Autonomy — local-first agent substrate"
  homepage "https://github.com/trustedautonomy/ta"
  version "0.1.0-alpha"
  license "Apache-2.0"

  # Source URL (required even with bottles)
  url "https://github.com/trustedautonomy/ta/archive/refs/tags/v0.1.0-alpha.tar.gz"
  sha256 "SOURCE_TARBALL_SHA256"

  # Pre-compiled bottles
  bottle do
    root_url "https://github.com/trustedautonomy/ta/releases/download/v0.1.0-alpha"
    sha256 cellar: :any_skip_relocation, arm64_sonoma: "ARM64_SHA256"
    sha256 cellar: :any_skip_relocation, ventura: "X86_64_SHA256"
  end

  def install
    bin.install "ta"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/ta --version")
  end
end
```

Note: Setting up bottles is more complex and optional for v0.1.x.

## Testing Checklist

Before publishing a formula update:

- [ ] Formula installs on macOS Apple Silicon
- [ ] Formula installs on macOS Intel
- [ ] `ta --version` shows correct version
- [ ] `ta --help` works
- [ ] `ta run "test" --source .` works
- [ ] `brew audit --strict --online ta` passes
- [ ] `brew test ta` passes

## Homebrew Core (Future Goal)

Getting TA into [Homebrew/homebrew-core](https://github.com/Homebrew/homebrew-core) makes it installable with just `brew install ta` (no tap needed).

**Requirements:**
- Stable releases (no alpha/beta)
- 75+ stars on GitHub (shows community interest)
- No closed-source dependencies
- Passes `brew audit --strict --online`
- Well-maintained (no stale issues)

**Process:**
1. Get TA stable and widely used
2. Submit PR to homebrew-core with formula
3. Respond to maintainer feedback
4. Once merged, users can `brew install ta`

For now, maintain the custom tap and revisit homebrew-core after v1.0.

## Resources

- [Homebrew Formula Cookbook](https://docs.brew.sh/Formula-Cookbook)
- [How to Create and Maintain a Tap](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap)
- [Acceptable Formulae](https://docs.brew.sh/Acceptable-Formulae)
- [Brew Audit](https://docs.brew.sh/Brew-Livecheck)

## Template Formula (Copy-Paste Ready)

```ruby
# Formula/ta.rb
class Ta < Formula
  desc "Trusted Autonomy — local-first agent substrate"
  homepage "https://github.com/trustedautonomy/ta"
  version "VERSION_HERE"
  license "Apache-2.0"

  # macOS Apple Silicon (arm64)
  if Hardware::CPU.arm?
    url "https://github.com/trustedautonomy/ta/releases/download/vVERSION_HERE/ta-vVERSION_HERE-aarch64-apple-darwin.tar.gz"
    sha256 "ARM64_SHA256_HERE"
  else
    # macOS Intel (x86_64)
    url "https://github.com/trustedautonomy/ta/releases/download/vVERSION_HERE/ta-vVERSION_HERE-x86_64-apple-darwin.tar.gz"
    sha256 "X86_64_SHA256_HERE"
  end

  def install
    bin.install "ta"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/ta --version")
  end
end
```

## Next Steps

When ready to implement Homebrew support:

1. Create `trustedautonomy/homebrew-tap` repository
2. Create initial formula using template above
3. Test on both Intel and Apple Silicon Macs
4. Document tap in main README
5. Consider automated formula updates via GitHub Actions
6. After v1.0: Submit to homebrew-core for wider distribution
