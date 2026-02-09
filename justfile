# Default recipe: run all checks then tests
default: check test

# Build all crates in the workspace
build:
    cargo build --workspace

# Run all tests
test:
    cargo nextest run --workspace 2>/dev/null || cargo test --workspace

# Run tests with standard cargo test (needed for doc tests, which nextest doesn't support)
test-doc:
    cargo test --workspace --doc

# Check formatting + linting (fails on any warning)
check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings

# Auto-format all code
fmt:
    cargo fmt --all

# Run a specific crate's tests (usage: just test-crate ta-audit)
test-crate CRATE:
    cargo nextest run -p {{CRATE}} 2>/dev/null || cargo test -p {{CRATE}}

# Verify everything before committing (format, lint, build, test)
verify: check build test
