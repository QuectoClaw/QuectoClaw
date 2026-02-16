pub mod app;
pub mod ui;

use crate::config::Config;
use crate::tui::app::{LogLevel, TuiState};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::CrosstermBackend;
use ratatui::Terminal;
use std::io;

/// Run the TUI dashboard loop in the context of the current terminal.
/// This will take over the terminal and block until the user quits.
pub async fn run(state: TuiState, cfg: Config) -> anyhow::Result<()> {
    // Count enabled channels
    let mut channel_count = 0;
    if cfg.channels.telegram.enabled {
        channel_count += 1;
    }
    if cfg.channels.discord.enabled {
        channel_count += 1;
    }
    if cfg.channels.slack.enabled {
        channel_count += 1;
    }
    state.set_active_channels(channel_count).await;

    // Push initial log if empty
    state
        .push_log(LogLevel::Info, "Dashboard started".to_string())
        .await;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let start_time = std::time::Instant::now();

    // Main loop
    loop {
        // Update uptime
        state.update_uptime(start_time.elapsed().as_secs()).await;

        // Collect state for rendering
        let logs = state.get_logs().await;
        let sessions = state.get_sessions().await;
        let stats = state.get_stats().await;

        // Draw
        terminal.draw(|frame| {
            ui::render(frame, &logs, &sessions, &stats);
        })?;

        // Poll for events (200ms timeout for smooth animation)
        if event::poll(std::time::Duration::from_millis(200))? {
            if let Ok(Event::Key(key)) = event::read() {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Char('c') => {
                        // In a real app we might clear logs, for now just push a log
                        state.push_log(LogLevel::Info, "Command Log: Clear logs (not fully implemented)".to_string()).await;
                    }
                    _ => {}
                }
            }
        }

        // Check if we should stop from external source
        if !state.is_running().await {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;

    Ok(())
}
