use crate::event::{AppEvent, EventHandler};
use crate::git::Repository;
use anyhow::Result;
use ratatui::{
    backend::CrosstermBackend,
    layout::Alignment,
    widgets::Paragraph,
    Terminal,
};
use std::io::Stdout;

pub struct App {
    running: bool,
    #[allow(dead_code)]
    repo: Repository,
}

impl App {
    pub fn new(repo: Repository) -> Self {
        Self { running: true, repo }
    }

    pub fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        let events = EventHandler::new();

        while self.running {
            terminal.draw(|frame| {
                let area = frame.area();
                let text = Paragraph::new("gitpeek — press Ctrl-C to quit")
                    .alignment(Alignment::Center);
                frame.render_widget(text, area);
            })?;

            match events.next()? {
                AppEvent::Key(key) => self.handle_key(key),
                AppEvent::Resize(_, _) => {}
                AppEvent::Tick => {}
            }
        }

        Ok(())
    }

    fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::{KeyCode, KeyModifiers};
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
            self.running = false;
        }
    }
}
