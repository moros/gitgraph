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

/// Stub for Phase 4: returns full diff content for a single file.
pub fn file_diff(_parent: &CommitHash, _commit: &CommitHash, _filepath: &str) -> FileDiff {
    FileDiff::default()
}
