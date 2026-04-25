use std::{
    fs::File,
    io::{self, BufRead, BufReader, stdout},
};

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
    widgets::{List, ListItem, Paragraph},
};

use xapi_viewer::{LogLine, parse_line};

struct App {
    file_path: String,
    lines: Vec<LogLine>,
    scroll_offset: usize,
}

impl App {
    // TODO:
    //   - We are loading the entire file into memory. Ok for small file but not for Giga ones.
    //   - Better handling error. Currently we stop if an error occured while reading
    fn new(path: String) -> io::Result<Self> {
        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        let lines: Vec<LogLine> = reader
            .lines()
            .map_while(Result::ok)
            .map(parse_line)
            .collect();

        Ok(Self {
            file_path: path,
            lines,
            scroll_offset: 0,
        })
    }

    fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    fn scroll_down(&mut self) {
        // Don't scroll past the end
        if self.scroll_offset < self.lines.len().saturating_sub(1) {
            self.scroll_offset += 1;
        }
    }
}

fn main() -> io::Result<()> {
    // Parse args: read path from command line
    let mut args = std::env::args();
    let path = match args.nth(1) {
        None => {
            eprintln!("Usage: xapi-viewer <path>");
            std::process::exit(1);
        }
        Some(p) => p,
    };

    // Load file before entering TUI mode
    let mut app = App::new(path)?;

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

            let top_bar = Paragraph::new(format!(
                "xapi-viewer - {} ({}/{})",
                app.file_path,
                app.scroll_offset + 1,
                app.lines.len()
            ))
            .style(bar_style);

            let bottom_bar = Paragraph::new("q=quit  j/↓=down  k/↑=up").style(bar_style);

            let items: Vec<ListItem> = app
                .lines
                .iter()
                .skip(app.scroll_offset)
                .take(chunks[1].height as usize) // only as many as fit on screen
                .map(|log_line| ListItem::new(log_line.raw.as_str()))
                .collect();

            let main_area = List::new(items);

            frame.render_widget(top_bar, chunks[0]);
            frame.render_widget(main_area, chunks[1]);
            frame.render_widget(bottom_bar, chunks[2]);
        })?;

        // EVENT: block until a key is pressed (or terminal resize).
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('j') | KeyCode::Down => app.scroll_down(),
                KeyCode::Char('k') | KeyCode::Up => app.scroll_up(),
                _ => {} // ignore other keys
            }
        }
    }

    // --- TEARDOWN ---
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    Ok(())
}
