use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub enum AppEvent {
    Key(KeyEvent),
    Resize(u16, u16),
    Tick,
}

pub struct EventHandler {
    receiver: mpsc::Receiver<AppEvent>,
}

impl EventHandler {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        let tick_rate = Duration::from_millis(250);

        thread::spawn(move || loop {
            if event::poll(tick_rate).expect("failed to poll events") {
                match event::read().expect("failed to read event") {
                    Event::Key(key) => {
                        let _ = sender.send(AppEvent::Key(key));
                    }
                    Event::Resize(w, h) => {
                        let _ = sender.send(AppEvent::Resize(w, h));
                    }
                    _ => {}
                }
            } else {
                let _ = sender.send(AppEvent::Tick);
            }
        });

        Self { receiver }
    }

    pub fn next(&self) -> Result<AppEvent> {
        Ok(self.receiver.recv()?)
    }
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new()
    }
}
