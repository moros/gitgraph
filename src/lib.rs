pub mod app;
pub mod color;
pub mod config;
pub mod event;
pub mod external;
pub mod git;
pub mod graph;
pub mod keybind;
pub mod protocol;
pub mod theme;
pub mod tree;
pub mod view;
pub mod widget;

use anyhow::Result;
use app::RunResult;
use config::Config;
use git::{CommitHash, FileChange, LogOrder, Repository};
use graph::calc_graph;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::Stdout;
use std::process::{Command, Stdio};
use std::rc::Rc;

pub fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, order: LogOrder) -> Result<()> {
    let use_text_graph = !terminal_supports_images();
    let mut restore_hash: Option<CommitHash> = None;

    loop {
        let config = Rc::new(Config::load()?);
        let batch_size = config.core.batch_size;
        let mut repo = Repository::load_partial(order, batch_size)?;
        let mut graph = calc_graph(&repo);

        loop {
            let events = event::EventHandler::new();
            let tx = events.sender();
            let uncommitted_files = detect_uncommitted_changes();

            let mut app = app::App::new(
                &repo,
                &graph,
                config.clone(),
                tx,
                restore_hash.as_ref(),
                use_text_graph,
                uncommitted_files,
            );

            match app.run(terminal, &events)? {
                RunResult::Quit => return Ok(()),
                RunResult::Refresh(hash) => {
                    restore_hash = hash;
                    break; // break inner loop → outer loop does full reload
                }
                RunResult::LoadMore(hash) => {
                    restore_hash = hash;
                    if batch_size > 0 && !repo.all_commits_loaded {
                        repo.load_more(batch_size)?;
                        graph = calc_graph(&repo);
                    }
                    // continue inner loop with expanded repo (or no-op if fully loaded)
                }
            }
        }
    }
}

/// Detect uncommitted working-tree changes (staged + unstaged + untracked).
///
/// Returns a list of [`FileChange`] entries for files that differ from HEAD.
/// Returns an empty Vec when the working tree is clean or when git is
/// unavailable.
fn detect_uncommitted_changes() -> Vec<FileChange> {
    // Use `git status --porcelain` to enumerate all changes.
    let Ok(output) = Command::new("git")
        .args(["status", "--porcelain"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    else {
        return vec![];
    };

    if !output.status.success() {
        return vec![];
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut files: Vec<FileChange> = Vec::new();

    for line in text.lines() {
        // Porcelain v1: "XY PATH" (X = index status, Y = worktree status)
        if line.len() < 3 {
            continue;
        }
        let x = line.chars().next().unwrap_or(' ');
        let y = line.chars().nth(1).unwrap_or(' ');
        let path = line[3..].trim().to_string();

        // Determine the primary status character (prefer index over worktree).
        let status = if x != ' ' && x != '?' { x } else { y };

        match status {
            'A' | '?' => files.push(FileChange::Add(path)),
            'M' | 'C' | 'U' => files.push(FileChange::Modify(path)),
            'D' => files.push(FileChange::Delete(path)),
            'R' => {
                // Porcelain v1 shows renames as "R  new -> old" (space-arrow-space).
                if let Some((new, old)) = path.split_once(" -> ") {
                    files.push(FileChange::Move(old.to_string(), new.to_string()));
                } else {
                    files.push(FileChange::Modify(path));
                }
            }
            _ => {}
        }
    }

    files
}

/// Returns `true` when the terminal is known to support inline image protocols
/// (iTerm2 OSC 1337 or Kitty graphics). Returns `false` to enable text-graph
/// fallback when running in a plain terminal.
fn terminal_supports_images() -> bool {
    std::env::var("KITTY_WINDOW_ID").is_ok()
        || std::env::var("TERM").is_ok_and(|t| t == "xterm-ghostty")
        || std::env::var("TERM_PROGRAM").is_ok_and(|t| t == "iTerm.app")
}
