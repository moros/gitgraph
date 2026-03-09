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
        let batch_size = config.core.batch_size;
        let mut repo = Repository::load_partial(order, batch_size)?;
        let mut graph = calc_graph(&repo);

        loop {
            let events = event::EventHandler::new();
            let tx = events.sender();

            let mut app = app::App::new(
                &repo,
                &graph,
                config.clone(),
                tx,
                restore_hash.as_ref(),
                use_text_graph,
            );

            match app.run(terminal, &events)? {
                RunResult::Quit => return Ok(()),
                RunResult::Refresh(hash) => {
                    restore_hash = hash;
                    break; // break inner loop → outer loop does full reload
                }
                RunResult::LoadMore(hash) => {
                    restore_hash = hash;
                    if batch_size > 0 && !repo.all_loaded() {
                        repo.load_more(batch_size)?;
                        graph = calc_graph(&repo);
                    }
                    // continue inner loop with expanded repo (or no-op if fully loaded)
                }
            }
        }
    }
}

/// Returns `true` when the terminal is known to support inline image protocols
/// (iTerm2 OSC 1337 or Kitty graphics). Returns `false` to enable text-graph
/// fallback when running in a plain terminal.
fn terminal_supports_images() -> bool {
    std::env::var("KITTY_WINDOW_ID").is_ok()
        || std::env::var("TERM").is_ok_and(|t| t == "xterm-ghostty")
        || std::env::var("TERM_PROGRAM").is_ok_and(|t| t == "iTerm.app")
}
