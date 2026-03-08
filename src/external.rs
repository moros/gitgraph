//! External diff tool integration for gitpeek.
//!
//! Supports two modes:
//! - **Pager**: pipe `git diff` output through a pager (e.g. `delta`, `diff-so-fancy`)
//! - **External diff**: run diff via `git -c diff.external=<cmd>` (e.g. `difftastic`)
//!
//! Template variables `{{width}}` and `{{columnWidth}}` are substituted before
//! invoking commands so callers can pass terminal dimensions.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::git::Ref;

use crate::git::commit::CommitHash;
use crate::git::diff::FileDiff;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for external diff tooling.
#[derive(Debug, Clone, Default)]
pub struct DiffToolConfig {
    /// Command string for a pager that processes `git diff` output (e.g. `"delta --dark"`).
    /// Empty string means no pager.
    pub pager_command: String,
    /// Command string for an external diff driver (e.g. `"difft --color=always"`).
    /// When set, takes precedence over `pager_command`.
    pub external_command: String,
    /// `--color` argument passed to `git diff` (e.g. `"always"`, `"never"`, `"auto"`).
    /// Defaults to `"always"`.
    pub color_arg: String,
}

impl DiffToolConfig {
    pub fn new() -> Self {
        Self {
            color_arg: "always".to_string(),
            ..Default::default()
        }
    }

    pub fn has_pager(&self) -> bool {
        !self.pager_command.trim().is_empty()
    }

    pub fn has_external(&self) -> bool {
        !self.external_command.trim().is_empty()
    }
}

// ---------------------------------------------------------------------------
// Template substitution
// ---------------------------------------------------------------------------

const WIDTH_MARKER: &str = "{{width}}";
const COLUMN_WIDTH_MARKER: &str = "{{columnWidth}}";

/// Substitute `{{width}}` and `{{columnWidth}}` in a command string.
fn apply_template(cmd: &str, width: u16) -> String {
    let half = width / 2;
    cmd.replace(WIDTH_MARKER, &width.to_string())
        .replace(COLUMN_WIDTH_MARKER, &half.to_string())
}

/// Split a command string (with possible arguments) into argv, applying template variables.
fn split_command(cmd: &str, width: u16) -> Vec<String> {
    apply_template(cmd, width)
        .split_whitespace()
        .map(|s| s.to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Pager
// ---------------------------------------------------------------------------

/// Pipe `diff_content` through `pager_cmd` and return the rendered output.
///
/// The pager process receives the diff on stdin and its stdout is captured.
/// Returns the raw pager output, or `diff_content` unchanged on failure.
pub fn execute_pager(diff_content: &str, pager_cmd: &str, width: u16) -> String {
    let argv = split_command(pager_cmd, width);
    if argv.is_empty() {
        return diff_content.to_string();
    }

    let mut child = match Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return diff_content.to_string(),
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(diff_content.as_bytes());
    }

    match child.wait_with_output() {
        Ok(output) => String::from_utf8_lossy(&output.stdout).into_owned(),
        Err(_) => diff_content.to_string(),
    }
}

// ---------------------------------------------------------------------------
// External diff
// ---------------------------------------------------------------------------

/// Run `git -c diff.external=<external_cmd> diff parent..commit -- filepath`
/// and return the tool's output as a `FileDiff`.
///
/// Returns an empty `FileDiff` on error.
pub fn execute_external_diff(
    repo_path: &Path,
    parent: &CommitHash,
    commit: &CommitHash,
    filepath: &str,
    external_cmd: &str,
    width: u16,
) -> FileDiff {
    let cmd = apply_template(external_cmd, width);
    let is_initial = parent.as_str().is_empty();

    if is_initial {
        // External tools generally cannot handle initial commits; fall back to
        // the standard synthesised diff from the data layer.
        return crate::git::diff::file_diff(repo_path, parent, commit, filepath);
    }

    let output = Command::new("git")
        .arg("-c")
        .arg(format!("diff.external={cmd}"))
        .arg("diff")
        .arg(format!("{}..{}", parent.as_str(), commit.as_str()))
        .arg("--")
        .arg(filepath)
        .current_dir(repo_path)
        .output();

    match output {
        Ok(o) => FileDiff {
            filename: filepath.to_string(),
            content: String::from_utf8_lossy(&o.stdout).into_owned(),
            ..Default::default()
        },
        Err(_) => FileDiff {
            filename: filepath.to_string(),
            ..Default::default()
        },
    }
}

// ---------------------------------------------------------------------------
// User command execution
// ---------------------------------------------------------------------------

const USER_CMD_MARKER_PREFIX: &str = "{{";
const USER_CMD_TARGET_HASH: &str = "{{target_hash}}";
const USER_CMD_FIRST_PARENT_HASH: &str = "{{first_parent_hash}}";
const USER_CMD_PARENT_HASHES: &str = "{{parent_hashes}}";
const USER_CMD_REFS: &str = "{{refs}}";
const USER_CMD_BRANCHES: &str = "{{branches}}";
const USER_CMD_REMOTE_BRANCHES: &str = "{{remote_branches}}";
const USER_CMD_TAGS: &str = "{{tags}}";
const USER_CMD_AREA_WIDTH: &str = "{{area_width}}";
const USER_CMD_AREA_HEIGHT: &str = "{{area_height}}";

/// All context needed for template substitution in a user command.
pub struct ExternalCommandParameters {
    pub command: Vec<String>,
    pub target_hash: String,
    pub parent_hashes: Vec<String>,
    pub all_refs: Vec<String>,
    pub branches: Vec<String>,
    pub remote_branches: Vec<String>,
    pub tags: Vec<String>,
    pub area_width: u16,
    pub area_height: u16,
}

impl ExternalCommandParameters {
    /// Build parameters from a commit info and refs.
    pub fn from_commit(
        command: Vec<String>,
        target_hash: String,
        parent_hashes: Vec<String>,
        refs: &[Ref],
        area_width: u16,
        area_height: u16,
    ) -> Self {
        let branches: Vec<String> = refs
            .iter()
            .filter(|r| matches!(r, Ref::Branch { .. }))
            .map(|r| r.name().to_string())
            .collect();
        let remote_branches: Vec<String> = refs
            .iter()
            .filter(|r| matches!(r, Ref::RemoteBranch { .. }))
            .map(|r| r.name().to_string())
            .collect();
        let tags: Vec<String> = refs
            .iter()
            .filter(|r| matches!(r, Ref::Tag { .. }))
            .map(|r| r.name().to_string())
            .collect();
        let all_refs: Vec<String> = refs
            .iter()
            .filter(|r| !matches!(r, Ref::Stash { .. }))
            .map(|r| r.name().to_string())
            .collect();
        Self {
            command,
            target_hash,
            parent_hashes,
            all_refs,
            branches,
            remote_branches,
            tags,
            area_width,
            area_height,
        }
    }
}

/// Execute a user command with template substitution, returning captured stdout.
pub fn exec_user_command(params: ExternalCommandParameters) -> Result<String, String> {
    if params.command.is_empty() {
        return Err("Empty command".to_string());
    }

    let mut argv: Vec<String> = Vec::with_capacity(params.command.len());
    for arg in &params.command {
        match arg.as_str() {
            // Standalone markers expand into multiple args so spaces in values work correctly.
            USER_CMD_BRANCHES => argv.extend(params.branches.clone()),
            USER_CMD_REMOTE_BRANCHES => argv.extend(params.remote_branches.clone()),
            USER_CMD_TAGS => argv.extend(params.tags.clone()),
            USER_CMD_REFS => argv.extend(params.all_refs.clone()),
            USER_CMD_PARENT_HASHES => argv.extend(params.parent_hashes.clone()),
            _ => argv.push(replace_user_cmd_arg(arg, &params)),
        }
    }

    if argv.is_empty() {
        return Err("Command expanded to empty argv".to_string());
    }

    let output = Command::new(&argv[0])
        .args(&argv[1..])
        .output()
        .map_err(|e| format!("Failed to execute '{}': {e}", argv[0]))?;

    if !output.status.success() {
        return Err(format!(
            "Command exited with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn replace_user_cmd_arg(s: &str, params: &ExternalCommandParameters) -> String {
    if !s.contains(USER_CMD_MARKER_PREFIX) {
        return s.to_string();
    }
    let sep = " ";
    let first_parent = params.parent_hashes.first().map(|s| s.as_str()).unwrap_or("");
    s.replace(USER_CMD_TARGET_HASH, &params.target_hash)
        .replace(USER_CMD_FIRST_PARENT_HASH, first_parent)
        .replace(USER_CMD_PARENT_HASHES, &params.parent_hashes.join(sep))
        .replace(USER_CMD_REFS, &params.all_refs.join(sep))
        .replace(USER_CMD_BRANCHES, &params.branches.join(sep))
        .replace(USER_CMD_REMOTE_BRANCHES, &params.remote_branches.join(sep))
        .replace(USER_CMD_TAGS, &params.tags.join(sep))
        .replace(USER_CMD_AREA_WIDTH, &params.area_width.to_string())
        .replace(USER_CMD_AREA_HEIGHT, &params.area_height.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_template_width() {
        let cmd = "delta --width={{width}}";
        let result = apply_template(cmd, 120);
        assert_eq!(result, "delta --width=120");
    }

    #[test]
    fn test_apply_template_column_width() {
        let cmd = "difft --display=side-by-side --width={{columnWidth}}";
        let result = apply_template(cmd, 120);
        assert_eq!(result, "difft --display=side-by-side --width=60");
    }

    #[test]
    fn test_split_command() {
        let parts = split_command("delta --dark --side-by-side", 80);
        assert_eq!(parts, vec!["delta", "--dark", "--side-by-side"]);
    }

    #[test]
    fn test_execute_pager_passthrough_on_invalid_cmd() {
        let content = "some diff content";
        let result = execute_pager(content, "nonexistent_pager_command_xyz", 80);
        // Should return content unchanged when pager fails to spawn
        assert_eq!(result, content);
    }

    #[test]
    fn test_diff_tool_config_defaults() {
        let cfg = DiffToolConfig::new();
        assert!(!cfg.has_pager());
        assert!(!cfg.has_external());
        assert_eq!(cfg.color_arg, "always");
    }

    #[test]
    fn test_diff_tool_config_has_pager() {
        let cfg = DiffToolConfig {
            pager_command: "delta".to_string(),
            ..DiffToolConfig::new()
        };
        assert!(cfg.has_pager());
        assert!(!cfg.has_external());
    }

    #[test]
    fn test_diff_tool_config_has_external() {
        let cfg = DiffToolConfig {
            external_command: "difft".to_string(),
            ..DiffToolConfig::new()
        };
        assert!(!cfg.has_pager());
        assert!(cfg.has_external());
    }
}
