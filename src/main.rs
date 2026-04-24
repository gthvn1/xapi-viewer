use std::io::{self, stdout};

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

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
            // For now, we render nothing, just an empty frame
            // The frame is still valid, ratatui will clear the screen for us.
            let _area = frame.area();
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
