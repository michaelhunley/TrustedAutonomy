//! Pluggable merge tool abstraction for three-way merges.
//!
//! `MergeTool` replaces the hard-coded `git merge-file` call in `three_way_merge()`
//! with a configurable strategy. The default is `Diff3MergeTool` (wraps git merge-file).

use std::path::Path;

/// Result of a three-way merge attempt.
#[derive(Debug)]
pub struct MergeResult {
    /// Whether the merge completed without conflicts.
    pub clean: bool,
    /// Number of conflict markers (0 = clean).
    pub conflicts: usize,
    /// Merged content (may contain conflict markers if `!clean`).
    pub content: Vec<u8>,
}

/// Pluggable merge algorithm.
pub trait MergeTool: Send + Sync {
    /// Perform a three-way merge.
    ///
    /// Arguments:
    /// - `base`: the common ancestor content
    /// - `ours`: our side of the merge
    /// - `theirs`: their side of the merge
    ///
    /// Returns `MergeResult` describing whether the merge was clean.
    fn merge(&self, base: &[u8], ours: &[u8], theirs: &[u8]) -> std::io::Result<MergeResult>;

    /// Display name for logging.
    fn name(&self) -> &str;
}

/// Diff3-based merge tool using `git merge-file`.
///
/// This is the default merge algorithm. It uses git's diff3 algorithm to
/// produce a standard three-way merge with conflict markers.
pub struct Diff3MergeTool;

impl MergeTool for Diff3MergeTool {
    fn merge(&self, base: &[u8], ours: &[u8], theirs: &[u8]) -> std::io::Result<MergeResult> {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut ours_file = NamedTempFile::new()?;
        let mut base_file = NamedTempFile::new()?;
        let mut theirs_file = NamedTempFile::new()?;

        ours_file.write_all(ours)?;
        base_file.write_all(base)?;
        theirs_file.write_all(theirs)?;

        // Flush so git can read the content.
        ours_file.flush()?;
        base_file.flush()?;
        theirs_file.flush()?;

        // git merge-file modifies ours_file in-place.
        // Exit code: 0 = clean, positive = conflict count.
        let status = std::process::Command::new("git")
            .args([
                "merge-file",
                "--quiet",
                ours_file.path().to_str().unwrap_or(""),
                base_file.path().to_str().unwrap_or(""),
                theirs_file.path().to_str().unwrap_or(""),
            ])
            .status()?;

        let conflicts = status.code().unwrap_or(-1).max(0) as usize;
        let content = std::fs::read(ours_file.path())?;

        Ok(MergeResult {
            clean: conflicts == 0,
            conflicts,
            content,
        })
    }

    fn name(&self) -> &str {
        "diff3"
    }
}

/// Agent-assisted merge tool (LLM-based conflict resolution).
///
/// Falls back to `Diff3MergeTool` when no LLM is available.
/// Full implementation deferred to v0.16.x.
pub struct AgentMergeTool {
    fallback: Diff3MergeTool,
}

impl AgentMergeTool {
    pub fn new() -> Self {
        Self {
            fallback: Diff3MergeTool,
        }
    }
}

impl Default for AgentMergeTool {
    fn default() -> Self {
        Self::new()
    }
}

impl MergeTool for AgentMergeTool {
    fn merge(&self, base: &[u8], ours: &[u8], theirs: &[u8]) -> std::io::Result<MergeResult> {
        // Full LLM-based resolution deferred to v0.16.x; fall back to diff3.
        self.fallback.merge(base, ours, theirs)
    }

    fn name(&self) -> &str {
        "agent"
    }
}

/// Take-source merge tool: always takes the source (ours) side.
///
/// Used when the user wants to accept all incoming changes without merging.
pub struct NoneMergeTool;

impl MergeTool for NoneMergeTool {
    fn merge(&self, _base: &[u8], ours: &[u8], _theirs: &[u8]) -> std::io::Result<MergeResult> {
        Ok(MergeResult {
            clean: true,
            conflicts: 0,
            content: ours.to_vec(),
        })
    }

    fn name(&self) -> &str {
        "none"
    }
}

/// Select a `MergeTool` implementation by name.
///
/// Supported values: `"diff3"` (default), `"agent"`, `"none"`.
/// Unknown names fall back to `"diff3"` with a warning.
pub fn select_merge_tool(name: &str) -> Box<dyn MergeTool> {
    match name {
        "diff3" | "" => Box::new(Diff3MergeTool),
        "agent" => Box::new(AgentMergeTool::new()),
        "none" => Box::new(NoneMergeTool),
        other => {
            tracing::warn!(
                tool = %other,
                "unknown merge tool '{}'; falling back to 'diff3'",
                other
            );
            Box::new(Diff3MergeTool)
        }
    }
}

/// Unused parameter to satisfy trait object sizing.
#[allow(dead_code)]
fn _assert_path_unused(_p: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff3_merge_tool_name() {
        assert_eq!(Diff3MergeTool.name(), "diff3");
    }

    #[test]
    fn agent_merge_tool_name() {
        assert_eq!(AgentMergeTool::new().name(), "agent");
    }

    #[test]
    fn none_merge_tool_takes_ours() {
        let tool = NoneMergeTool;
        let result = tool.merge(b"base", b"ours", b"theirs").unwrap();
        assert_eq!(result.content, b"ours");
        assert!(result.clean);
        assert_eq!(result.conflicts, 0);
    }

    #[test]
    fn select_merge_tool_returns_diff3_for_unknown() {
        let tool = select_merge_tool("bogus");
        assert_eq!(tool.name(), "diff3");
    }

    #[test]
    fn select_merge_tool_diff3() {
        let tool = select_merge_tool("diff3");
        assert_eq!(tool.name(), "diff3");
    }

    #[test]
    fn select_merge_tool_none() {
        let tool = select_merge_tool("none");
        assert_eq!(tool.name(), "none");
    }
}
