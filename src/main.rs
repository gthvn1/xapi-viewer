use std::{
    fs::File,
    io::{self, BufRead, BufReader, stdout},
};

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
};

use xapi_viewer::{LogLine, PatternKind, parse_line};

struct App {
    file_path: String,
    lines: Vec<LogLine>,
    scroll_offset: usize,
    pending_g: bool, // used to track 'g' pressed twice
    visible_height: usize,
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
            pending_g: false,
            visible_height: 0, // will be updated each render
        })
    }

    fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
    }

    fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.lines.len().saturating_sub(1);
    }

    fn scroll_up_by(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    fn scroll_down_by(&mut self, n: usize) {
        // Don't scroll past the end
        if self.scroll_offset < self.lines.len().saturating_sub(n) {
            self.scroll_offset += n;
        }
    }
}

fn color_for(kind: PatternKind) -> Color {
    match kind {
        PatternKind::TaskId => Color::Yellow,
        PatternKind::RequestId => Color::Cyan,
        PatternKind::TrackId => Color::Magenta,
        PatternKind::Uuid => Color::Green,
        PatternKind::OpaqueRef => Color::Red,
    }
}

fn render_log_line(log_line: &LogLine) -> ListItem<'_> {
    let mut cursor = 0;
    let mut spans = Vec::new();

    for m in &log_line.matches {
        if m.range.start > cursor {
            spans.push(Span::raw(&log_line.raw[cursor..m.range.start]));
        }
        spans.push(Span::styled(
            &log_line.raw[m.range.clone()], // Range isn't Copy, need clone
            color_for(m.kind),
        ));
        cursor = m.range.end;
    }

    if cursor < log_line.raw.len() {
        spans.push(Span::raw(&log_line.raw[cursor..log_line.raw.len()]));
    }

    let line = Line::from(spans);
    ListItem::new(line)
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
            let bar_style = Style::default().bg(Color::LightGreen).fg(Color::Black);

            let top_bar = Paragraph::new(format!(
                "xapi-viewer - {} ({}/{})",
                app.file_path,
                app.scroll_offset + 1,
                app.lines.len()
            ))
            .style(bar_style);

            let bottom_bar =
                Paragraph::new("q=quit  j/k=line  Ctrl-u/d=half  PgUp/PgDn=page  gg/G=top/bot")
                    .style(bar_style);

            let items: Vec<ListItem> = app
                .lines
                .iter()
                .skip(app.scroll_offset)
                .take(chunks[1].height as usize) // only as many as fit on screen
                .map(render_log_line)
                .collect();

            let main_area = List::new(items);

            frame.render_widget(top_bar, chunks[0]);
            frame.render_widget(main_area, chunks[1]);
            frame.render_widget(bottom_bar, chunks[2]);
        })?;

        // Read terminal size outside the closure, so no need to mutate app in it.
        let term_size = terminal.size()?;
        app.visible_height = (term_size.height as usize).saturating_sub(2); // minus top + bottom

        // EVENT: block until a key is pressed (or terminal resize).
        if let Event::Key(key) = event::read()? {
            let full_page = app.visible_height;
            let half_page = full_page / 2;

            // Handle 'g' specially (multi-key sequence). Continues early.
            if key.code == KeyCode::Char('g') {
                if app.pending_g {
                    app.scroll_to_top();
                    app.pending_g = false;
                } else {
                    app.pending_g = true;
                }
                continue;
            }

            // Any other key resets the pending state and falls through to the match.
            app.pending_g = false;
            match (key.code, key.modifiers) {
                (KeyCode::Char('q'), _) => break,
                // Scroll down
                (KeyCode::Char('j'), _) | (KeyCode::Down, _) => app.scroll_down_by(1),
                (KeyCode::PageDown, _) => app.scroll_down_by(full_page),
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => app.scroll_down_by(half_page),
                // Scroll up
                (KeyCode::Char('k'), _) | (KeyCode::Up, _) => app.scroll_up_by(1),
                (KeyCode::PageUp, _) => app.scroll_up_by(full_page),
                (KeyCode::Char('u'), KeyModifiers::CONTROL) => app.scroll_up_by(half_page),
                // Scroll top and bottom
                (KeyCode::Home, _) => app.scroll_to_top(),
                (KeyCode::Char('G'), _) | (KeyCode::End, _) => app.scroll_to_bottom(),
                // ignore other keys
                _ => {}
            }
        }
    }

    // --- TEARDOWN ---
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    Ok(())
}
