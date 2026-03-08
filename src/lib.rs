pub mod app;
pub mod color;
pub mod config;
pub mod event;
pub mod git;
pub mod graph;
pub mod keybind;
pub mod protocol;
pub mod theme;
pub mod tree;
pub mod view;
pub mod widget;

use anyhow::Result;
use config::Config;
use git::{LogOrder, Repository};
use graph::calc_graph;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::Stdout;
use std::rc::Rc;

pub fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, order: LogOrder) -> Result<()> {
    let config = Rc::new(Config::load()?);
    let repo = Repository::load(order)?;
    let graph = calc_graph(&repo);

    let events = event::EventHandler::new();
    let tx = events.sender();

    let mut app = app::App::new(&repo, &graph, config, tx);
    app.run(terminal, &events)
}
