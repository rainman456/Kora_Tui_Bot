use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use std::time::Duration;
use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyModifiers, MouseEventKind};

use super::state::Action;

/// Events that can occur in the TUI
#[derive(Debug)]
pub enum Event {
    /// Terminal tick (for rendering)
    Tick,
    /// Key press
    Key(KeyCode, KeyModifiers),
    /// Mouse event
    Mouse(MouseEventKind, u16, u16),
    /// Terminal resize
    Resize(u16, u16),
    /// State update action
    Action(Action),
}

/// Event handler for the TUI
pub struct EventLoop {
    action_tx: UnboundedSender<Action>,
    action_rx: UnboundedReceiver<Action>,
    event_rx: UnboundedReceiver<Event>,
}

impl EventLoop {
    pub fn new() -> Self {
        let (action_tx, action_rx) = unbounded_channel();
        let (event_tx, event_rx) = unbounded_channel();
        
        // Spawn terminal event listener
        tokio::spawn(async move {
            loop {
                // Poll for events with 33ms timeout (30 FPS)
                if event::poll(Duration::from_millis(33)).unwrap_or(false) {
                    match event::read() {
                        Ok(CrosstermEvent::Key(key_event)) => {
                            let _ = event_tx.send(Event::Key(key_event.code, key_event.modifiers));
                        }
                        Ok(CrosstermEvent::Mouse(mouse_event)) => {
                            let _ = event_tx.send(Event::Mouse(mouse_event.kind, mouse_event.column, mouse_event.row));
                        }
                        Ok(CrosstermEvent::Resize(width, height)) => {
                            let _ = event_tx.send(Event::Resize(width, height));
                        }
                        _ => {}
                    }
                } else {
                    // Timeout expired - send tick
                    let _ = event_tx.send(Event::Tick);
                }
            }
        });
        
        Self {
            action_tx,
            action_rx,
            event_rx,
        }
    }
    
    /// Get a sender for dispatching actions
    pub fn get_action_sender(&self) -> UnboundedSender<Action> {
        self.action_tx.clone()
    }
    
    /// Receive the next event
    pub async fn next(&mut self) -> Option<Event> {
        tokio::select! {
            // Priority to actions from background tasks
            action = self.action_rx.recv() => {
                action.map(Event::Action)
            }
            // Then terminal events
            event = self.event_rx.recv() => {
                event
            }
        }
    }
}

/// Command dispatcher for keyboard inputs
pub enum Command {
    Quit,
    Scan,
    DryRun,
    Execute,
    Export,
    Whitelist,
    NavigateUp,
    NavigateDown,
    PageUp,
    PageDown,
    Refresh,
    ToggleMode,
    ToggleHelp,
    ToggleExpanded,
    MouseScrollUp,
    MouseScrollDown,
}

impl Command {
    pub fn from_key(code: KeyCode, modifiers: KeyModifiers) -> Option<Self> {
        // Ctrl+C always quits
        if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
            return Some(Self::Quit);
        }
        
        match code {
            KeyCode::Char('q') | KeyCode::Esc => Some(Self::Quit),
            KeyCode::Char('s') => Some(Self::Scan),
            KeyCode::Char('d') => Some(Self::DryRun),
            KeyCode::Char('e') => Some(Self::Execute),
            KeyCode::Char('x') => Some(Self::Export),
            KeyCode::Char('w') => Some(Self::Whitelist),
            KeyCode::Char('r') => Some(Self::Refresh),
            KeyCode::Char('m') => Some(Self::ToggleMode),
            KeyCode::Char('?') => Some(Self::ToggleHelp),
            KeyCode::Char('E') => Some(Self::ToggleExpanded),
            KeyCode::Up | KeyCode::Char('k') => Some(Self::NavigateUp),
            KeyCode::Down | KeyCode::Char('j') => Some(Self::NavigateDown),
            KeyCode::PageUp => Some(Self::PageUp),
            KeyCode::PageDown => Some(Self::PageDown),
            _ => None,
        }
    }
    
    pub fn from_mouse(kind: MouseEventKind) -> Option<Self> {
        match kind {
            MouseEventKind::ScrollUp => Some(Self::MouseScrollUp),
            MouseEventKind::ScrollDown => Some(Self::MouseScrollDown),
            _ => None,
        }
    }
}