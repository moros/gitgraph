pub mod app;
pub mod event;
pub mod git;
pub mod graph;
pub mod tree;
pub mod view;
pub mod widget;

use anyhow::Result;
use git::{LogOrder, Repository};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::Stdout;

pub fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, order: LogOrder) -> Result<()> {
    let repo = Repository::load(order)?;
    let mut app = app::App::new(repo);
    app.run(terminal)
}
