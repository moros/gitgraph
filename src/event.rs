use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub enum AppEvent {
    Key(KeyEvent),
    Resize(u16, u16),
    Tick,
    Quit,
    OpenDetail,
    OpenRefs,
    OpenHelp,
    CloseHelp,
    /// Open user command view for the given slot (1–9).
    OpenUserCommand(u8),
    CloseUserCommand,
    /// Close the detail view and return to the list view.
    CloseDetail,
    /// Close the refs view and return to the list view.
    CloseRefs,
    /// Navigate to the next commit while staying in detail view.
    DetailNextCommit,
    /// Navigate to the previous commit while staying in detail view.
    DetailPrevCommit,
    CopyToClipboard {
        name: String,
        value: String,
    },
    ClearStatusLine,
    UpdateStatusInput(String, Option<u16>, Option<String>),
    NotifyInfo(String),
    NotifyWarn(String),
    NotifyError(String),
    NotifySuccess(String),
    /// Reload repository data while preserving user context.
    Refresh,
    /// Scroll reached near the bottom; request loading more commits.
    LoadMore,
    /// Terminal focus lost/gained events
    FocusLost,
    FocusGained,
}

#[derive(Clone)]
pub struct Sender {
    tx: mpsc::Sender<AppEvent>,
}

impl Sender {
    pub fn send(&self, event: AppEvent) {
        let _ = self.tx.send(event);
    }
}

pub struct EventHandler {
    receiver: mpsc::Receiver<AppEvent>,
    sender: Sender,
}

impl EventHandler {
    pub fn new() -> Self {
        let (tx, receiver) = mpsc::channel();
        let sender = Sender { tx: tx.clone() };
        let tick_rate = Duration::from_millis(250);

        thread::spawn(move || loop {
            if event::poll(tick_rate).expect("failed to poll events") {
                match event::read().expect("failed to read event") {
                    Event::Key(key) => {
                        let _ = tx.send(AppEvent::Key(key));
                    }
                    Event::Resize(w, h) => {
                        let _ = tx.send(AppEvent::Resize(w, h));
                    }
                    Event::FocusLost => {
                        let _ = tx.send(AppEvent::FocusLost);
                    }
                    Event::FocusGained => {
                        let _ = tx.send(AppEvent::FocusGained);
                    }
                    _ => {}
                }
            } else {
                let _ = tx.send(AppEvent::Tick);
            }
        });

        Self { receiver, sender }
    }

    pub fn sender(&self) -> Sender {
        self.sender.clone()
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
