pub mod app;
pub mod event;
pub mod git;
pub mod graph;
pub mod tree;
pub mod view;
pub mod widget;

use anyhow::Result;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::Stdout;

pub fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    let mut app = app::App::new();
    app.run(terminal)
}
