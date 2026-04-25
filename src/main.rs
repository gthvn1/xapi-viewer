use std::io::{self, stdout};

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::Paragraph,
};

fn main() -> io::Result<()> {
    // Parse args: read path from command line
    let mut args = std::env::args();
    let _path = match args.nth(1) {
        None => {
            eprintln!("Usage: xapi-viewer <path>");
            std::process::exit(1);
        }
        Some(p) => p,
    };

    // --- SETUP ---
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // --- Main Loop ---
    loop {
        // DRAW: redraw the whole screen
        terminal.draw(|frame| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // top bar: exactly 1 row
                    Constraint::Min(0),    // middle: whatever is left (at least 0)
                    Constraint::Length(1), // bottom bar: exactly 1 row
                ])
                .split(frame.area());

            // Style is Copy so using it doesn't move ownership.
            let bar_style = Style::default().bg(Color::Blue).fg(Color::White);

            let top_bar = Paragraph::new("xapi-viewer").style(bar_style);
            let main_area = Paragraph::new("Middle placeholder");
            let bottom_bar = Paragraph::new("q=quit").style(bar_style);

            frame.render_widget(top_bar, chunks[0]);
            frame.render_widget(main_area, chunks[1]);
            frame.render_widget(bottom_bar, chunks[2]);
        })?;

        // EVENT: block until a key is pressed (or terminal resize).
        if let Event::Key(key) = event::read()?
            && key.code == KeyCode::Char('q')
        {
            break;
        }
    }

    // --- TEARDOWN ---
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    Ok(())
}
