use std::{
    collections::HashSet,
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
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
};

use xapi_viewer::{LogLine, PatternKind, first_match_idx, last_match_idx, parse_line};

const TOP_BAR_HEIGHT: u16 = 1;
const BOTTOM_BAR_HEIGHT: u16 = 2;

/// Application state for the TUI.
///
/// # Invariants
/// - `visible_lines` contains indices into `lines` that pass `active_filters`.
///   When `active_filters` is empty, `visible_lines == (0..lines.len()).collect()`.
/// - `scroll_offset` indexes into `visible_lines`, NOT `lines` directly.
///   Reset to 0 when filters change.
/// - `selected` references absolute line indices into `lines`. May refer to a line not
///   currently in `visible_lines` if filters changed: rendering handles this gracefully
///   (no inverse-video drawn for hidden lines).
struct App {
    file_path: String,
    /// The full parsed file. Never mutated after construction. Order matters.
    lines: Vec<LogLine>,

    /// Indices into `lines` that pass `active_filters`. Recomputed by `recompute_visible`
    /// whenever filters change.
    visible_lines: Vec<usize>,

    /// Index into `visible_lines` of the top visible row. Reset to 0 when filters change.
    scroll_offset: usize,

    /// Used to track 'g' pressed twice.
    pending_g: bool,

    /// This is the size of the `main_area`.
    visible_height: usize,

    /// Currently selected match: `(absolute_line_idx, match_idx_within_line)`.
    /// `None` means nothing selected. Cleared when filters change.
    selected: Option<(usize, usize)>,

    /// User-added filter tokens. OR semantics: a line is visible if it contains any of these
    /// substrings.
    active_filters: HashSet<String>,
}

impl App {
    // TODO:
    //   - We are loading the entire file into memory. Ok for small file but not for Giga ones.
    //   - Better error handling. Currently we stop if an error occurred while reading
    fn new(path: String) -> io::Result<Self> {
        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        let lines: Vec<LogLine> = reader
            .lines()
            .map_while(Result::ok)
            .map(parse_line)
            .collect();
        // When app starts there is no filters, all lines are visible
        let visible_lines = (0..lines.len()).collect();

        Ok(Self {
            file_path: path,
            lines,
            scroll_offset: 0,
            pending_g: false,
            visible_height: 0, // will be updated each render
            selected: None,
            active_filters: HashSet::new(),
            visible_lines,
        })
    }

    fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
        eprintln!("set scroll_offset to {}", self.scroll_offset);
    }

    fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.visible_lines.len().saturating_sub(1);
        eprintln!("set scroll_offset to {}", self.scroll_offset);
    }

    fn scroll_up_by(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
        eprintln!("set scroll_offset to {}", self.scroll_offset);
    }

    fn scroll_down_by(&mut self, n: usize) {
        // Don't scroll past the end
        if self.scroll_offset < self.visible_lines.len().saturating_sub(n) {
            self.scroll_offset += n;
        }
        eprintln!("set scroll_offset to {}", self.scroll_offset);
    }

    fn ensure_selected_visible(&mut self) {
        let Some((line_idx, _)) = self.selected else {
            eprintln!("nothing is selected");
            return;
        };

        // Here line_idx is an index in self.lines.
        // For example we can have visible_lines = [3, 47, 333]
        // and user selects 333. We need to find where it is in visible_lines.
        //
        // TODO: position is O(n) and won't scale well. Implement a HashMap<usize,usize>
        // to map absolute -> visible position when needed.
        let Some(pos) = self.visible_lines.iter().position(|&i| i == line_idx) else {
            eprintln!(
                "Is it a bug? selected line_idx {} is not part of visible lines",
                line_idx
            );
            return;
        };

        eprintln!(
            "ensure_selected_visible: pos={} scroll_offset={} visible_height={}",
            pos, self.scroll_offset, self.visible_height
        );

        // scroll_offset is an index into visible_lines, so we can compare it
        // to pos.
        if pos < self.scroll_offset {
            self.scroll_offset = pos; // scrolled too far down
        } else if pos >= self.scroll_offset + self.visible_height {
            self.scroll_offset = pos.saturating_sub(self.visible_height.saturating_sub(1));
        }
    }

    // Select and ensure that selection is visible
    fn select(&mut self, sel: (usize, usize)) {
        self.selected = Some(sel);
        eprintln!("selected set to {:?}", sel);
        self.ensure_selected_visible();
    }

    fn select_next_match(&mut self, kind: Option<PatternKind>) {
        let (line_idx, match_idx) = match self.selected {
            Some((line_idx, match_idx)) => (line_idx, match_idx),
            None => {
                // Nothing selected yet, pick first match. For first-time selection we
                // anchor to viewport.
                for &line_idx in self.visible_lines[self.scroll_offset..].iter() {
                    if let Some(idx) = first_match_idx(&self.lines[line_idx].matches, kind) {
                        self.select((line_idx, idx));
                        return;
                    }
                }

                // Wrap and try from the beginning
                for &line_idx in self.visible_lines[0..self.scroll_offset].iter() {
                    if let Some(idx) = first_match_idx(&self.lines[line_idx].matches, kind) {
                        self.select((line_idx, idx));
                        return;
                    }
                }

                return; // no matches now in visible area, do nothing
            }
        };

        // It is an existing selection, find next match.
        if let Some(idx) = first_match_idx(&self.lines[line_idx].matches[match_idx + 1..], kind) {
            self.select((line_idx, match_idx + 1 + idx));
            return;
        }

        // We don't find a match on line_idx, try next ones
        let Some(pos) = self.visible_lines.iter().position(|&i| i == line_idx) else {
            eprintln!(
                "Is it a bug? failed to find line_idx {} in select_next_match",
                line_idx
            );
            return;
        };

        for &next_line in self.visible_lines[pos + 1..].iter() {
            if let Some(idx) = first_match_idx(&self.lines[next_line].matches, kind) {
                self.select((next_line, idx));
                return;
            }
        }

        // We reach the end of visible_lines and we don't find anything. Wrap from beginning.
        for &next_line in self.visible_lines[..pos].iter() {
            if let Some(idx) = first_match_idx(&self.lines[next_line].matches, kind) {
                self.select((next_line, idx));
                return;
            }
        }
    }

    fn select_prev_match(&mut self, kind: Option<PatternKind>) {
        let (line_idx, match_idx) = match self.selected {
            Some((line_idx, match_idx)) => (line_idx, match_idx),
            None => {
                for &line_idx in self.visible_lines[..self.scroll_offset].iter().rev() {
                    if let Some(idx) = last_match_idx(&self.lines[line_idx].matches, kind) {
                        self.select((line_idx, idx));
                        return;
                    }
                }
                // Wrap and try from the end to scroll offset
                for &line_idx in self.visible_lines[self.scroll_offset..].iter().rev() {
                    if let Some(idx) = last_match_idx(&self.lines[line_idx].matches, kind) {
                        self.select((line_idx, idx));
                        return;
                    }
                }

                return;
            }
        };

        // Find previous match on the current line
        if let Some(idx) = last_match_idx(&self.lines[line_idx].matches[..match_idx], kind) {
            self.select((line_idx, idx));
            return;
        }

        // We don't find a match on line_idx, try previous ones
        let Some(pos) = self.visible_lines.iter().position(|&i| i == line_idx) else {
            eprintln!(
                "Is it a bug? failed to find line_idx {} in select_prev_match",
                line_idx
            );
            return;
        };

        for &prev_line in self.visible_lines[..pos].iter().rev() {
            if let Some(idx) = last_match_idx(&self.lines[prev_line].matches, kind) {
                self.select((prev_line, idx));
                return;
            }
        }

        for &prev_line in self.visible_lines[pos + 1..].iter().rev() {
            if let Some(idx) = last_match_idx(&self.lines[prev_line].matches, kind) {
                self.select((prev_line, idx));
                return;
            }
        }
    }

    fn clear_selection(&mut self) {
        self.selected = None;
        self.recompute_visible();
    }

    fn clear_filters(&mut self) {
        self.active_filters.clear();
        self.clear_selection();
    }

    fn recompute_visible(&mut self) {
        if self.active_filters.is_empty() {
            // No filters: every line is visible
            self.visible_lines = (0..self.lines.len()).collect();
        } else {
            // For each line we check if any of the filters belongs to the line
            // TODO: it will probably not scale for million lines of logs...
            self.visible_lines = self
                .lines
                .iter()
                .enumerate()
                .filter(|(_, log_line)| {
                    self.active_filters
                        .iter()
                        .any(|f| log_line.raw.contains(f.as_str()))
                })
                .map(|(idx, _)| idx)
                .collect();
        }

        // Let's print the first 10 values of visible lines
        let lim = self.visible_lines.len().min(10);
        let slice = &self.visible_lines[..lim];
        eprintln!("first indices of visible_lines: {:?}", slice);

        // Reset scroll offset.
        // Note: We deliberately KEEP self.selected if it is still visible.
        // The user can press Enter again to remove that filter (toggle behavior).
        self.scroll_offset = 0;
        if let Some((current_selected_line, _)) = self.selected
            && !self.visible_lines.contains(&current_selected_line)
        {
            self.selected = None;
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

fn render_log_line(log_line: &LogLine, selected_match_idx: Option<usize>) -> ListItem<'_> {
    let mut cursor = 0;
    let mut spans = Vec::new();

    for (i, m) in log_line.matches.iter().enumerate() {
        // The style of the match depends if it is selected or not
        let style = if Some(i) == selected_match_idx {
            Style::default()
                .fg(color_for(m.kind))
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(color_for(m.kind))
        };

        if m.range.start > cursor {
            spans.push(Span::raw(&log_line.raw[cursor..m.range.start]));
        }

        spans.push(Span::styled(
            &log_line.raw[m.range.clone()], // Range isn't Copy, need clone
            style,
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
                    Constraint::Length(TOP_BAR_HEIGHT),
                    Constraint::Min(0),    // middle: whatever is left (at least 0)
                    Constraint::Length(BOTTOM_BAR_HEIGHT),
                ])
                .split(frame.area());

            // Style is Copy so using it doesn't move ownership.
            let bar_style = Style::default().bg(Color::LightGreen).fg(Color::Black);

            let top_text = if app.active_filters.is_empty() {
                format!(
                    "xapi-viewer - {} ({}/{})",
                    app.file_path,
                    app.scroll_offset + 1,
                    app.lines.len())
            } else {
                format!(
                    "xapi-viewer - {} ({}/{}) filtered, {} active",
                    app.file_path,
                    app.scroll_offset + 1,
                    app.visible_lines.len(),
                    app.active_filters.len())
            };

            let top_bar = Paragraph::new(top_text).style(bar_style);

            let bottom_bar =
                Paragraph::new(
                    "q=quit  j/k=scroll  Ctrl-j/k=half  PgUp/Dn=page  gg/G=top/bot\n\
                    Tab/S-Tab=match  d/r/t/u/o=next-kind  D/R/T/U/O=prev-kind  Enter=filter  x=clear-filters  Esc=unsel")
                    .style(bar_style);

            let items: Vec<ListItem> = app
                .visible_lines
                .iter()
                .skip(app.scroll_offset)
                .take(chunks[1].height as usize) // only as many as fit on screen
                .map(|&abs_idx| {
                    let log_line = &app.lines[abs_idx];
                    let selected_match_idx = match app.selected {
                        Some((sel_line, sel_match)) if sel_line == abs_idx => Some(sel_match),
                        _ => None,
                    };
                    render_log_line(log_line, selected_match_idx)
                })
                .collect();

            let main_area = List::new(items);

            frame.render_widget(top_bar, chunks[0]);
            frame.render_widget(main_area, chunks[1]);
            frame.render_widget(bottom_bar, chunks[2]);
        })?;

        // Read terminal size outside the closure, so no need to mutate app in it.
        let term_size = terminal.size()?;
        app.visible_height = (term_size.height as usize)
            .saturating_sub(TOP_BAR_HEIGHT as usize + BOTTOM_BAR_HEIGHT as usize);

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

                // clear filters
                (KeyCode::Char('x'), _) => app.clear_filters(),

                // Scroll down (half_page, one line, page)
                (KeyCode::Char('j'), KeyModifiers::CONTROL) => app.scroll_down_by(half_page),
                (KeyCode::Down, KeyModifiers::CONTROL) => app.scroll_down_by(half_page),
                (KeyCode::Char('j'), _) | (KeyCode::Down, _) => app.scroll_down_by(1),
                (KeyCode::PageDown, _) => app.scroll_down_by(full_page),

                // Scroll up
                (KeyCode::Char('k'), KeyModifiers::CONTROL) => app.scroll_up_by(half_page),
                (KeyCode::Up, KeyModifiers::CONTROL) => app.scroll_up_by(half_page),
                (KeyCode::Char('k'), _) | (KeyCode::Up, _) => app.scroll_up_by(1),
                (KeyCode::PageUp, _) => app.scroll_up_by(full_page),

                // Scroll top and bottom
                (KeyCode::Home, _) => app.scroll_to_top(),
                (KeyCode::Char('G'), _) | (KeyCode::End, _) => app.scroll_to_bottom(),

                // Select/Unselect matches
                (KeyCode::Esc, _) => app.clear_selection(),
                (KeyCode::Tab, _) => app.select_next_match(None),
                (KeyCode::BackTab, _) => app.select_prev_match(None),
                (KeyCode::Char('d'), _) => app.select_next_match(Some(PatternKind::TaskId)),
                (KeyCode::Char('r'), _) => app.select_next_match(Some(PatternKind::RequestId)),
                (KeyCode::Char('t'), _) => app.select_next_match(Some(PatternKind::TrackId)),
                (KeyCode::Char('u'), _) => app.select_next_match(Some(PatternKind::Uuid)),
                (KeyCode::Char('o'), _) => app.select_next_match(Some(PatternKind::OpaqueRef)),
                // As we are using SHIFT to go backward, caracters will be uppercase...
                (KeyCode::Char('D'), _) => app.select_prev_match(Some(PatternKind::TaskId)),
                (KeyCode::Char('R'), _) => app.select_prev_match(Some(PatternKind::RequestId)),
                (KeyCode::Char('T'), _) => app.select_prev_match(Some(PatternKind::TrackId)),
                (KeyCode::Char('U'), _) => app.select_prev_match(Some(PatternKind::Uuid)),
                (KeyCode::Char('O'), _) => app.select_prev_match(Some(PatternKind::OpaqueRef)),

                // Toggle match in active filters
                (KeyCode::Enter, _) => {
                    if let Some((line_idx, match_idx)) = app.selected {
                        let log_line = &app.lines[line_idx];
                        let m = &log_line.matches[match_idx];
                        let token = log_line.raw[m.range.clone()].to_string();

                        if app.active_filters.contains(&token) {
                            eprintln!("{} removed from active filters", &token);
                            app.active_filters.remove(&token);
                        } else {
                            eprintln!("{} added in active filters", &token);
                            app.active_filters.insert(token);
                        }
                        app.recompute_visible();
                    } else {
                        eprintln!("You must select a tag before adding it in active filters");
                    }
                }
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
