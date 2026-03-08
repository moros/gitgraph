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
use git::{CommitHash, LogOrder, Repository};
use graph::calc_graph;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::Stdout;
use std::rc::Rc;

pub fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, order: LogOrder) -> Result<()> {
    let use_text_graph = !terminal_supports_images();
    let mut restore_hash: Option<CommitHash> = None;

    loop {
        let config = Rc::new(Config::load()?);
        let repo = Repository::load(order)?;
        let graph = calc_graph(&repo);

        let events = event::EventHandler::new();
        let tx = events.sender();

        let mut app = app::App::new(
            &repo,
            &graph,
            config,
            tx,
            restore_hash.as_ref(),
            use_text_graph,
        );

        match app.run(terminal, &events)? {
            RunResult::Quit => break,
            RunResult::Refresh(hash) => {
                restore_hash = hash;
                continue;
            }
        }
    }

    Ok(())
}

/// Returns `true` when the terminal is known to support inline image protocols
/// (iTerm2 OSC 1337 or Kitty graphics). Returns `false` to enable text-graph
/// fallback when running in a plain terminal.
fn terminal_supports_images() -> bool {
    std::env::var("KITTY_WINDOW_ID").is_ok()
        || std::env::var("TERM").is_ok_and(|t| t == "xterm-ghostty")
        || std::env::var("TERM_PROGRAM").is_ok_and(|t| t == "iTerm.app")
}
