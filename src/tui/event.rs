use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, MouseEvent};
use std::time::Duration;

pub enum Event {
    /// Terminal tick
    Tick,
    /// Key press
    Key(KeyEvent),
    /// Mouse event
    Mouse(MouseEvent),
    /// Terminal resize
    Resize(u16, u16),
}

pub struct EventHandler {
    /// Event receiver
    receiver: tokio::sync::mpsc::Receiver<Event>,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(100);
        
        // Spawn event listener
        tokio::spawn(async move {
            let mut last_tick = tokio::time::Instant::now();
            
            loop {
                let timeout = tick_rate
                    .checked_sub(last_tick.elapsed())
                    .unwrap_or(Duration::from_secs(0));
                
                if event::poll(timeout).unwrap() {
                    match event::read().unwrap() {
                        CrosstermEvent::Key(key) => {
                            sender.send(Event::Key(key)).await.ok();
                        }
                        CrosstermEvent::Mouse(mouse) => {
                            sender.send(Event::Mouse(mouse)).await.ok();
                        }
                        CrosstermEvent::Resize(width, height) => {
                            sender.send(Event::Resize(width, height)).await.ok();
                        }
                        _ => {}
                    }
                }
                
                if last_tick.elapsed() >= tick_rate {
                    sender.send(Event::Tick).await.ok();
                    last_tick = tokio::time::Instant::now();
                }
            }
        });
        
        Self { receiver }
    }
    
    pub async fn next(&mut self) -> Option<Event> {
        self.receiver.recv().await
    }
}