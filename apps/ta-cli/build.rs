// build.rs — Embed git and build metadata into the ta binary.
//
// Sets these env vars at compile time:
//   TA_GIT_HASH     — short git commit hash (e.g., "abc1234"), or "unknown"
//   TA_BUILD_DATE   — build date in YYYY-MM-DD format

use std::process::Command;

fn main() {
    // Get the short git hash.
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Check for uncommitted changes.
    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);

    let hash_suffix = if dirty {
        format!("{}-dirty", git_hash)
    } else {
        git_hash
    };

    // Build date.
    let build_date = chrono_free_date();

    println!("cargo:rustc-env=TA_GIT_HASH={}", hash_suffix);
    println!("cargo:rustc-env=TA_BUILD_DATE={}", build_date);

    // Re-run if git HEAD changes (new commits).
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/");
}

/// Get current date as YYYY-MM-DD without pulling in chrono.
fn chrono_free_date() -> String {
    // Use the `date` command (available on macOS/Linux).
    Command::new("date")
        .args(["+%Y-%m-%d"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
