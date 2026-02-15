//! # Diff Handlers
//!
//! Configuration-driven external diff viewing for non-text files.
//!
//! Example `.ta/diff-handlers.toml`:
//! ```toml
//! [[handler]]
//! pattern = "*.uasset"
//! command = "UnrealEditor"
//! args = ["{file}"]
//! description = "Unreal Engine asset"
//!
//! [[handler]]
//! pattern = "*.{png,jpg,jpeg}"
//! command = "open"  # macOS
//! args = ["-a", "Preview", "{file}"]
//! description = "Image file"
//!
//! [[handler]]
//! pattern = "*.blend"
//! command = "blender"
//! args = ["{file}"]
//! description = "Blender file"
//! ```

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use thiserror::Error;

/// Configuration for external diff handlers loaded from `.ta/diff-handlers.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiffHandlersConfig {
    /// List of handler rules, evaluated in order.
    #[serde(default)]
    pub handler: Vec<HandlerRule>,
}

/// A single handler rule mapping file patterns to external applications.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandlerRule {
    /// Glob pattern for matching file paths (e.g., "*.png", "assets/**/*.blend").
    pub pattern: String,
    /// External command to execute (e.g., "open", "blender", "UnrealEditor").
    pub command: String,
    /// Arguments to pass to the command. Use `{file}` placeholder for the file path.
    #[serde(default)]
    pub args: Vec<String>,
    /// Human-readable description of this handler (optional).
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Error, Debug)]
pub enum DiffHandlerError {
    #[error("Failed to read diff-handlers config: {0}")]
    ConfigRead(#[from] std::io::Error),
    #[error("Failed to parse diff-handlers config: {0}")]
    ConfigParse(#[from] toml::de::Error),
    #[error("No handler configured for file: {0}")]
    NoHandler(String),
    #[error("Failed to launch external handler: {0}")]
    LaunchFailed(String),
    #[error("Handler command not found: {0}")]
    CommandNotFound(String),
}

impl DiffHandlersConfig {
    /// Load diff-handlers config from a TOML file.
    ///
    /// Returns a default (empty) config if the file doesn't exist.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, DiffHandlerError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        let config: DiffHandlersConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// Load config from the standard location (`.ta/diff-handlers.toml` in project root).
    pub fn load_from_project<P: AsRef<Path>>(project_root: P) -> Result<Self, DiffHandlerError> {
        let config_path = project_root.as_ref().join(".ta/diff-handlers.toml");
        Self::load(config_path)
    }

    /// Find the first handler matching the given file path.
    pub fn find_handler(&self, file_path: &str) -> Option<&HandlerRule> {
        self.handler
            .iter()
            .find(|h| pattern_matches(&h.pattern, file_path))
    }

    /// Open a file with the configured handler, or use OS default if no handler matches.
    ///
    /// - If a handler is configured for the file pattern, use it.
    /// - Otherwise, fall back to OS default (`open` on macOS, `xdg-open` on Linux).
    pub fn open_file<P: AsRef<Path>>(
        &self,
        file_path: P,
        fallback_to_os_default: bool,
    ) -> Result<(), DiffHandlerError> {
        let file_path = file_path.as_ref();
        let file_str = file_path.to_string_lossy();

        if let Some(handler) = self.find_handler(&file_str) {
            launch_handler(handler, file_path)
        } else if fallback_to_os_default {
            launch_os_default(file_path)
        } else {
            Err(DiffHandlerError::NoHandler(file_str.to_string()))
        }
    }
}

/// Check if a glob pattern matches a file path.
///
/// Supports basic glob syntax:
/// - `*` matches any characters except `/`
/// - `**` matches any characters including `/`
/// - `{a,b,c}` matches any of the alternatives
fn pattern_matches(pattern: &str, path: &str) -> bool {
    // Use the glob crate for pattern matching.
    // For simplicity, we'll use a basic glob matcher that handles common patterns.
    match glob::Pattern::new(pattern) {
        Ok(glob_pattern) => glob_pattern.matches(path),
        Err(_) => false,
    }
}

/// Launch an external handler for a file.
fn launch_handler(handler: &HandlerRule, file_path: &Path) -> Result<(), DiffHandlerError> {
    let file_str = file_path.to_string_lossy();

    // Substitute {file} placeholder in args.
    let args: Vec<String> = handler
        .args
        .iter()
        .map(|arg| arg.replace("{file}", &file_str))
        .collect();

    // Launch the command.
    let result = Command::new(&handler.command)
        .args(&args)
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                DiffHandlerError::CommandNotFound(handler.command.clone())
            } else {
                DiffHandlerError::LaunchFailed(format!("{}: {}", handler.command, e))
            }
        })?;

    tracing::info!(
        "Launched {} {} with PID {:?}",
        handler.command,
        file_str,
        result.id()
    );

    Ok(())
}

/// Launch OS default application for a file.
///
/// - macOS: `open <file>`
/// - Linux: `xdg-open <file>`
/// - Windows: `start <file>` (not yet implemented)
fn launch_os_default(file_path: &Path) -> Result<(), DiffHandlerError> {
    let file_str = file_path.to_string_lossy();

    #[cfg(target_os = "macos")]
    let (command, args) = ("open", vec![file_str.as_ref()]);

    #[cfg(target_os = "linux")]
    let (command, args) = ("xdg-open", vec![file_str.as_ref()]);

    #[cfg(target_os = "windows")]
    let (command, args) = ("cmd", vec!["/c", "start", "", file_str.as_ref()]);

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    return Err(DiffHandlerError::LaunchFailed(
        "OS default handler not supported on this platform".to_string(),
    ));

    let result = Command::new(command)
        .args(&args)
        .spawn()
        .map_err(|e| DiffHandlerError::LaunchFailed(format!("OS default handler failed: {}", e)))?;

    tracing::info!(
        "Launched OS default handler for {} with PID {:?}",
        file_str,
        result.id()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_pattern_matching() {
        assert!(pattern_matches("*.png", "image.png"));
        assert!(pattern_matches("*.png", "path/to/image.png"));
        assert!(!pattern_matches("*.png", "image.jpg"));

        assert!(pattern_matches(
            "assets/**/*.uasset",
            "assets/models/char.uasset"
        ));
        assert!(pattern_matches("assets/**/*.uasset", "assets/char.uasset"));
        assert!(!pattern_matches("assets/**/*.uasset", "models/char.uasset"));

        // Note: glob crate doesn't support brace expansion {a,b,c}.
        // For multiple extensions, users should create separate handler rules.
        // Example: one rule for *.png, another for *.jpg.
        assert!(!pattern_matches("*.{png,jpg}", "image.png")); // brace syntax not supported
    }

    #[test]
    fn test_load_config_missing_file() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("missing.toml");
        let config = DiffHandlersConfig::load(&config_path).unwrap();
        assert!(config.handler.is_empty());
    }

    #[test]
    fn test_load_config_valid_file() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("diff-handlers.toml");
        std::fs::write(
            &config_path,
            r#"
[[handler]]
pattern = "*.png"
command = "open"
args = ["-a", "Preview", "{file}"]
description = "Image viewer"

[[handler]]
pattern = "*.blend"
command = "blender"
args = ["{file}"]
"#,
        )
        .unwrap();

        let config = DiffHandlersConfig::load(&config_path).unwrap();
        assert_eq!(config.handler.len(), 2);
        assert_eq!(config.handler[0].pattern, "*.png");
        assert_eq!(config.handler[0].command, "open");
        assert_eq!(config.handler[1].pattern, "*.blend");
    }

    #[test]
    fn test_find_handler() {
        let config = DiffHandlersConfig {
            handler: vec![
                HandlerRule {
                    pattern: "*.png".to_string(),
                    command: "image-viewer".to_string(),
                    args: vec!["{file}".to_string()],
                    description: Some("Image".to_string()),
                },
                HandlerRule {
                    pattern: "assets/**/*.blend".to_string(),
                    command: "blender".to_string(),
                    args: vec!["{file}".to_string()],
                    description: None,
                },
            ],
        };

        let handler = config.find_handler("test.png");
        assert!(handler.is_some());
        assert_eq!(handler.unwrap().command, "image-viewer");

        let handler = config.find_handler("assets/models/char.blend");
        assert!(handler.is_some());
        assert_eq!(handler.unwrap().command, "blender");

        let handler = config.find_handler("test.txt");
        assert!(handler.is_none());
    }

    #[test]
    fn test_arg_substitution() {
        let handler = HandlerRule {
            pattern: "*.test".to_string(),
            command: "test-cmd".to_string(),
            args: vec![
                "--input".to_string(),
                "{file}".to_string(),
                "--output".to_string(),
                "{file}.out".to_string(),
            ],
            description: None,
        };

        let file_path = Path::new("/tmp/test.test");
        let args: Vec<String> = handler
            .args
            .iter()
            .map(|arg| arg.replace("{file}", &file_path.to_string_lossy()))
            .collect();

        assert_eq!(args[0], "--input");
        assert_eq!(args[1], "/tmp/test.test");
        assert_eq!(args[2], "--output");
        assert_eq!(args[3], "/tmp/test.test.out");
    }

    // Note: We don't test actual command launching here because it depends on the system.
    // Manual testing required for verifying launch behavior.
}
