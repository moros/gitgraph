use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::git::commit::CommitHash;

#[derive(Debug, Clone)]
pub enum FileChange {
    Add(String),
    Modify(String),
    Delete(String),
    Move(String, String),
}

#[derive(Debug, Clone, Default)]
pub struct FileDiff {
    pub filename: String,
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub content: String,
    pub added_lines: usize,
    pub removed_lines: usize,
}

pub fn get_diff_summary(path: &Path, commit_hash: &CommitHash) -> Vec<FileChange> {
    let mut cmd = Command::new("git")
        .arg("diff")
        .arg("--name-status")
        .arg(format!("{}^", commit_hash.as_str()))
        .arg(commit_hash.as_str())
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let stdout = cmd.stdout.take().expect("failed to open stdout");
    let reader = BufReader::new(stdout);

    let mut changes = Vec::new();

    for line in reader.lines() {
        let line = line.unwrap();
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.is_empty() {
            continue;
        }
        match &parts[0][0..1] {
            "A" if parts.len() >= 2 => changes.push(FileChange::Add(parts[1].into())),
            "M" if parts.len() >= 2 => changes.push(FileChange::Modify(parts[1].into())),
            "D" if parts.len() >= 2 => changes.push(FileChange::Delete(parts[1].into())),
            "R" if parts.len() >= 3 => {
                changes.push(FileChange::Move(parts[1].into(), parts[2].into()))
            }
            _ => {}
        }
    }

    cmd.wait().unwrap();

    changes
}

pub fn get_initial_commit_additions(path: &Path, commit_hash: &CommitHash) -> Vec<FileChange> {
    let mut cmd = Command::new("git")
        .arg("ls-tree")
        .arg("--name-only")
        .arg("-r")
        .arg(commit_hash.as_str())
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let stdout = cmd.stdout.take().expect("failed to open stdout");
    let reader = BufReader::new(stdout);

    let mut changes = Vec::new();

    for line in reader.lines() {
        let line = line.unwrap();
        if !line.is_empty() {
            changes.push(FileChange::Add(line));
        }
    }

    cmd.wait().unwrap();

    changes
}

/// Returns the full unified diff for a single file between parent and commit.
///
/// For initial commits (no parent / empty parent hash), synthesises a diff
/// showing every line as added via `git show commit:filepath`.
pub fn file_diff(
    path: &Path,
    parent: &CommitHash,
    commit: &CommitHash,
    filepath: &str,
) -> FileDiff {
    let is_initial = parent.as_str().is_empty();

    let raw = if is_initial {
        get_initial_file_content(path, commit, filepath)
    } else {
        get_diff_output(path, parent, commit, filepath)
    };

    parse_file_diff(filepath, &raw)
}

/// Run `git diff --color=never parent..commit -- filepath`.
fn get_diff_output(
    path: &Path,
    parent: &CommitHash,
    commit: &CommitHash,
    filepath: &str,
) -> String {
    let output = Command::new("git")
        .arg("diff")
        .arg("--color=never")
        .arg(format!("{}..{}", parent.as_str(), commit.as_str()))
        .arg("--")
        .arg(filepath)
        .current_dir(path)
        .output();

    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
        Err(_) => String::new(),
    }
}

/// For initial commits, synthesise a unified diff from `git show commit:filepath`.
fn get_initial_file_content(path: &Path, commit: &CommitHash, filepath: &str) -> String {
    let output = Command::new("git")
        .arg("show")
        .arg(format!("{}:{}", commit.as_str(), filepath))
        .current_dir(path)
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let content = String::from_utf8_lossy(&o.stdout).into_owned();
            let line_count = content.lines().count();
            let mut diff = format!(
                "diff --git a/{fp} b/{fp}\nnew file mode 100644\n--- /dev/null\n+++ b/{fp}\n@@ -0,0 +1,{n} @@\n",
                fp = filepath,
                n = line_count,
            );
            for line in content.lines() {
                diff.push('+');
                diff.push_str(line);
                diff.push('\n');
            }
            diff
        }
        _ => String::new(),
    }
}

/// Parse a raw unified diff string into a FileDiff, extracting stats and paths.
fn parse_file_diff(filepath: &str, raw: &str) -> FileDiff {
    let mut result = FileDiff {
        filename: filepath.to_string(),
        content: raw.to_string(),
        ..Default::default()
    };

    for line in raw.lines() {
        if let Some(stripped) = line.strip_prefix("--- ") {
            result.old_path = Some(stripped.to_string());
        } else if let Some(stripped) = line.strip_prefix("+++ ") {
            result.new_path = Some(stripped.to_string());
        } else if line.starts_with('+') && !line.starts_with("+++") {
            result.added_lines += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            result.removed_lines += 1;
        }
    }

    result
}

/// Get file changes for the working tree (staged + unstaged) compared to HEAD.
///
/// Uses `git diff --name-status HEAD` which shows all changes in both the
/// index and the working tree relative to HEAD.
pub fn get_uncommitted_diff_summary(path: &Path) -> Vec<FileChange> {
    let mut cmd = Command::new("git")
        .arg("diff")
        .arg("--name-status")
        .arg("HEAD")
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let stdout = cmd.stdout.take().expect("failed to open stdout");
    let reader = BufReader::new(stdout);

    let mut changes = Vec::new();

    for line in reader.lines() {
        let line = line.unwrap();
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.is_empty() {
            continue;
        }
        match &parts[0][0..1] {
            "A" if parts.len() >= 2 => changes.push(FileChange::Add(parts[1].into())),
            "M" if parts.len() >= 2 => changes.push(FileChange::Modify(parts[1].into())),
            "D" if parts.len() >= 2 => changes.push(FileChange::Delete(parts[1].into())),
            "R" if parts.len() >= 3 => {
                changes.push(FileChange::Move(parts[1].into(), parts[2].into()))
            }
            _ => {}
        }
    }

    cmd.wait().unwrap();

    changes
}

/// Returns the combined staged + unstaged diff for a single uncommitted file.
///
/// Tries `git diff HEAD -- filepath` first (covers staged + unstaged changes).
/// Falls back to `git diff --cached -- filepath` for files staged but not
/// yet present in HEAD (e.g. newly added files).
pub fn uncommitted_file_diff(path: &Path, filepath: &str) -> FileDiff {
    let output = Command::new("git")
        .arg("diff")
        .arg("HEAD")
        .arg("--color=never")
        .arg("--")
        .arg(filepath)
        .current_dir(path)
        .output();

    let raw = match output {
        Ok(o) if !o.stdout.is_empty() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => {
            let output = Command::new("git")
                .arg("diff")
                .arg("--cached")
                .arg("--color=never")
                .arg("--")
                .arg(filepath)
                .current_dir(path)
                .output();
            match output {
                Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
                Err(_) => String::new(),
            }
        }
    };

    parse_file_diff(filepath, &raw)
}

/// Get diff output rendered by an external diff tool (e.g. difftastic).
///
/// Uses `git -c diff.external=<tool> diff parent..commit -- filepath`.
/// Falls back to a synthesised diff for initial commits.
pub fn get_file_diff_with_tool(
    path: &Path,
    parent: &CommitHash,
    commit: &CommitHash,
    filepath: &str,
    external_cmd: &str,
) -> FileDiff {
    let is_initial = parent.as_str().is_empty();

    let raw = if is_initial {
        get_initial_file_content(path, commit, filepath)
    } else {
        let output = Command::new("git")
            .arg("-c")
            .arg(format!("diff.external={external_cmd}"))
            .arg("diff")
            .arg(format!("{}..{}", parent.as_str(), commit.as_str()))
            .arg("--")
            .arg(filepath)
            .current_dir(path)
            .output();

        match output {
            Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
            Err(_) => String::new(),
        }
    };

    FileDiff {
        filename: filepath.to_string(),
        content: raw,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_file_diff_stats() {
        let raw = "diff --git a/foo.rs b/foo.rs\n\
--- a/foo.rs\n\
+++ b/foo.rs\n\
@@ -1,3 +1,3 @@\n\
 fn main() {\n\
-    println!(\"Hello\");\n\
+    println!(\"Hello, World!\");\n\
 }\n";

        let diff = parse_file_diff("foo.rs", raw);
        assert_eq!(diff.filename, "foo.rs");
        assert_eq!(diff.added_lines, 1);
        assert_eq!(diff.removed_lines, 1);
        assert_eq!(diff.old_path, Some("a/foo.rs".to_string()));
        assert_eq!(diff.new_path, Some("b/foo.rs".to_string()));
    }

    #[test]
    fn test_parse_file_diff_empty() {
        let diff = parse_file_diff("bar.rs", "");
        assert_eq!(diff.filename, "bar.rs");
        assert_eq!(diff.added_lines, 0);
        assert_eq!(diff.removed_lines, 0);
        assert!(diff.old_path.is_none());
        assert!(diff.new_path.is_none());
    }

    #[test]
    fn test_uncommitted_file_diff_parses_stats() {
        // Simulate output from `git diff HEAD --color=never -- foo.rs`
        let raw = "diff --git a/foo.rs b/foo.rs\n\
--- a/foo.rs\n\
+++ b/foo.rs\n\
@@ -1,2 +1,3 @@\n\
 fn main() {}\n\
+// new comment\n\
-// old comment\n";

        let diff = parse_file_diff("foo.rs", raw);
        assert_eq!(diff.filename, "foo.rs");
        assert_eq!(diff.added_lines, 1);
        assert_eq!(diff.removed_lines, 1);
    }

    #[test]
    fn test_initial_diff_synthesised() {
        let mut diff = "diff --git a/test.rs b/test.rs\nnew file mode 100644\n--- /dev/null\n+++ b/test.rs\n@@ -0,0 +1,2 @@\n".to_string();
        diff.push_str("+line one\n+line two\n");

        let parsed = parse_file_diff("test.rs", &diff);
        assert_eq!(parsed.added_lines, 2);
        assert_eq!(parsed.removed_lines, 0);
        assert_eq!(parsed.new_path, Some("b/test.rs".to_string()));
    }
}
