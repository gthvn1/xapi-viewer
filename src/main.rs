use std::{
    borrow::Cow,
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
    text::{Line, Span, Text},
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

    /// When set to true, log lines may span multiple lines. It is false by default.
    wrap: bool,
}

impl App {
    /// Loads the log file at `path` into memory and returns an `App` ready to
    /// display it.
    ///
    /// Every line is parsed for identifier patterns via [`parse_line`].  All
    /// lines start out visible (no filters applied).  Returns an `io::Error`
    /// if the file cannot be opened or read.
    ///
    /// # Limitations
    /// - The entire file is held in memory; very large files may exhaust RAM.
    /// - Read errors cause the load to stop rather than skipping the bad line.
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
            wrap: false,
        })
    }

    /// Toggles whether long log lines wrap onto multiple terminal rows.
    fn toggle_wrap(&mut self) {
        self.wrap = !self.wrap;
    }

    /// Resets `scroll_offset` to 0, bringing the first visible line to the top
    /// of the viewport.
    fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
        eprintln!("set scroll_offset to {}", self.scroll_offset);
    }

    /// Sets `scroll_offset` to the last visible line, bringing the bottom of
    /// the log into view.
    fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.visible_lines.len().saturating_sub(1);
        eprintln!("set scroll_offset to {}", self.scroll_offset);
    }

    /// Scrolls the viewport up by `n` lines, clamping at the top so that
    /// `scroll_offset` never underflows.
    fn scroll_up_by(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
        eprintln!("set scroll_offset to {}", self.scroll_offset);
    }

    /// Scrolls the viewport down by `n` lines, stopping when the last visible
    /// line would scroll off screen.
    fn scroll_down_by(&mut self, n: usize) {
        // Don't scroll past the end
        if self.scroll_offset < self.visible_lines.len().saturating_sub(n) {
            self.scroll_offset += n;
        }
        eprintln!("set scroll_offset to {}", self.scroll_offset);
    }

    /// Adjusts `scroll_offset` so that the currently selected line is within
    /// the viewport.
    ///
    /// If the selection is above the viewport the offset is moved up; if it is
    /// below, the offset is moved down just enough to reveal the last selected
    /// row.  Does nothing when there is no active selection or when the
    /// selected line is not part of `visible_lines`.
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

    /// Sets the active selection to `sel` (an `(absolute_line_idx, match_idx)`
    /// pair) and scrolls the viewport if necessary to keep it on screen.
    fn select(&mut self, sel: (usize, usize)) {
        self.selected = Some(sel);
        eprintln!("selected set to {:?}", sel);
        self.ensure_selected_visible();
    }

    /// Moves the selection forward to the next match of the given `kind`, or
    /// the next match of any kind when `kind` is `None`.
    ///
    /// If nothing is currently selected the search starts from the top of the
    /// visible viewport. When the end of `visible_lines` is reached the
    /// search wraps around to the beginning. Does nothing if there are no
    /// qualifying matches in the current view.
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

                // Wrap and try from the beginning.
                for &line_idx in self.visible_lines[0..self.scroll_offset].iter() {
                    if let Some(idx) = first_match_idx(&self.lines[line_idx].matches, kind) {
                        self.select((line_idx, idx));
                        return;
                    }
                }

                return; // no matches now in visible area, do nothing.
            }
        };

        // It is an existing selection, find next match.
        if let Some(idx) = first_match_idx(&self.lines[line_idx].matches[match_idx + 1..], kind) {
            self.select((line_idx, match_idx + 1 + idx));
            return;
        }

        // We don't find a match on line_idx, try next ones.
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

    /// Moves the selection backward to the previous match of the given `kind`,
    /// or the previous match of any kind when `kind` is `None`.
    ///
    /// Mirrors the behaviour of [`select_next_match`] but traverses
    /// `visible_lines` in reverse and wraps from the beginning back to the
    /// end.
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
                // Wrap and try from the end to scroll offset.
                for &line_idx in self.visible_lines[self.scroll_offset..].iter().rev() {
                    if let Some(idx) = last_match_idx(&self.lines[line_idx].matches, kind) {
                        self.select((line_idx, idx));
                        return;
                    }
                }

                return;
            }
        };

        // Find previous match on the current line.
        if let Some(idx) = last_match_idx(&self.lines[line_idx].matches[..match_idx], kind) {
            self.select((line_idx, idx));
            return;
        }

        // We don't find a match on line_idx, try previous ones.
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

    /// Clears the active selection and recomputes the visible line set.
    fn clear_selection(&mut self) {
        self.selected = None;
        self.recompute_visible();
    }

    /// Removes all active filter tokens, making every line visible again, and
    /// clears the current selection.
    fn clear_filters(&mut self) {
        self.active_filters.clear();
        self.clear_selection();
    }

    /// Rebuilds `visible_lines` from `lines` according to `active_filters`.
    ///
    /// When `active_filters` is empty every line index is included. Otherwise
    /// a line is included if its raw text contains **any** of the filter
    /// tokens (OR semantics). `scroll_offset` is reset to 0, and
    /// `selected` is cleared if the previously selected line is no longer
    /// visible.
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

        // For debugging purpose, let's print the first 10 values of visible lines.
        let lim = self.visible_lines.len().min(10);
        let slice = &self.visible_lines[..lim];
        eprintln!("first 10 indices of visible_lines: {:?}", slice);

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

/// Maps a [`PatternKind`] to the TUI foreground colour used to highlight
/// matches of that kind.
fn color_for(kind: PatternKind) -> Color {
    match kind {
        PatternKind::TaskId => Color::Yellow,
        PatternKind::RequestId => Color::Cyan,
        PatternKind::TrackId => Color::Magenta,
        PatternKind::Uuid => Color::Green,
        PatternKind::OpaqueRef => Color::Red,
    }
}

/// Append `text` to the current row in `rows`.
/// When line wrapping is enabled, log lines may span multiple lines when
/// exceed `line_size` columns.
///
/// `col` is the caller's running column counter for the current row; it is
/// updated in place. Each new row created by wrapping starts with a
/// reverse-video blank prefix of `margin` columns to visually align with
/// the line-number prefix at the start of the first row.
///
/// The `text` parameter accepts both borrowed (`&str`) and owned (`String`)
/// inputs via `Cow`. This is required because some callers pass slices of
/// the original log line (borrowed) while others pass `format!`-built
/// strings (owned).
///
/// # Caveats
///
/// Lengths are counted in bytes (`str::len`), not display columns. This is
/// correct for ASCII log lines but will mis-wrap multi-byte UTF-8 input.
///
/// # FIXME: scroll math vs. wrapped rows
///
/// When wrapping is enabled, a single logical line in `App::visible_lines`
/// can render as N physical terminal rows. The viewport `take` count in
/// `main()` is in logical lines, so when wrapped lines are present the
/// rendered area overflows the available height and content at the bottom
/// is clipped by ratatui. `ensure_selected_visible` and `scroll_*_by` also
/// reason in logical lines, so scrolling can feel jumpy near wrapped lines.
///
/// A correct fix requires either (a) computing physical row height per
/// logical line and accumulating until height is exhausted, or (b)
/// switching the rendering model so visible_lines is row-indexed. Both are
/// non-trivial. For now, callers can disable wrapping (single-row mode) to
/// avoid the issue.
fn push_wrapped<'a>(
    rows: &mut Vec<Line<'a>>,
    col: &mut usize,
    text: impl Into<Cow<'a, str>>,
    style: Style,
    line_size: usize,
    margin: usize,
    wrap: bool,
) {
    let mut text: Cow<'a, str> = text.into();

    let prefix_blank = Span::styled(
        " ".repeat(margin),
        Style::default().add_modifier(Modifier::REVERSED),
    );

    if rows.is_empty() {
        rows.push(Line::from(Vec::<Span>::new()));
    }

    loop {
        // Check if the whole line fits into one line
        let avail = line_size - *col;
        if !wrap || text.len() <= avail {
            // Only update col if we need to wrap. Otherwise it will grow past
            // line_size and we will underflow (usize, so it panics).
            if wrap {
                *col += text.len();
            }
            rows.last_mut()
                .unwrap()
                .spans
                .push(Span::styled(text, style));
            return;
        }

        // Doesn't fit.
        let (head, tail) = text.split_at(avail);
        // Append the head,
        rows.last_mut()
            .unwrap()
            .spans
            .push(Span::styled(head.to_string(), style));
        // and start new row,
        rows.push(Line::from(vec![prefix_blank.clone()]));
        // continue with the tail.
        text = Cow::Owned(tail.to_string());
        *col = margin;
    }
}

/// Converts a [`LogLine`] into a coloured ratatui [`ListItem`].
///
/// The line number (1-based, drawn with reverse video) is prepended, then
/// each recognised match is coloured according to [`color_for`]. The match
/// at index `selected_match_idx`, if any, is additionally rendered with
/// the reverse-video modifier to indicate the current selection. Unmatched
/// text between spans is rendered in the default style.
fn render_log_line(
    log_line: &LogLine,
    selected_match_idx: Option<usize>,
    line_idx: usize,
    line_max_size: usize,
    wrap: bool,
) -> ListItem<'_> {
    let mut cursor = 0;
    let mut rows: Vec<Line> = Vec::new();
    let mut col = 0;
    let margin = 7;
    // Ensure that the width for printing line is at least 20
    let line_real_size = line_max_size.max(20);

    // First print the real line number
    push_wrapped(
        &mut rows,
        &mut col,
        format!("{:<width$}", line_idx + 1, width = margin),
        Style::default().add_modifier(Modifier::REVERSED),
        line_real_size,
        margin,
        wrap,
    );
    //spans.push(Span::styled(
    //    format!("{:<width$}", line_idx + 1, width = margin),
    //    Style::default().add_modifier(Modifier::REVERSED),
    //));

    // Now iterate through matches to color as needed
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
            push_wrapped(
                &mut rows,
                &mut col,
                &log_line.raw[cursor..m.range.start],
                Style::default(),
                line_real_size,
                margin,
                wrap,
            );
            //spans.push(Span::raw(&log_line.raw[cursor..m.range.start]));
        }

        push_wrapped(
            &mut rows,
            &mut col,
            &log_line.raw[m.range.clone()],
            style,
            line_real_size,
            margin,
            wrap,
        );
        //spans.push(Span::styled(
        //    &log_line.raw[m.range.clone()], // Range isn't Copy, need clone
        //    style,
        //));
        cursor = m.range.end;
    }

    // Don't forget the rest of the line
    if cursor < log_line.raw.len() {
        push_wrapped(
            &mut rows,
            &mut col,
            &log_line.raw[cursor..log_line.raw.len()],
            Style::default(),
            line_real_size,
            margin,
            wrap,
        );
        //spans.push(Span::raw(&log_line.raw[cursor..log_line.raw.len()]));
    }

    // TESTING TO ADD A NEW LINE
    //let prefix_blank = Span::styled(
    //    " ".repeat(margin),
    //    Style::default().add_modifier(Modifier::REVERSED),
    //);
    //let fakeline = Line::from(vec![prefix_blank, Span::raw("fake line")]);

    ListItem::new(Text::from(rows))
}

/// Entry point.
///
/// Reads the log file path from the first command-line argument, loads it
/// into an [`App`], then enters the ratatui TUI event loop.  The loop redraws
/// the screen on every iteration and blocks on a key event before updating
/// state.  The terminal is restored to its original state when the user quits
/// with `q`.
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
        // Read terminal size outside the closure, so no need to mutate app in it.
        let term_size = terminal.size()?;

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
                    "q=quit  j/k=scroll  Ctrl-j/k=half  PgUp/Dn=page  gg/G=top/bot w=toggle-long-lines\n\
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
                    render_log_line(log_line, selected_match_idx, abs_idx, term_size.width as usize, app.wrap)
                })
                .collect();

            let main_area = List::new(items);

            frame.render_widget(top_bar, chunks[0]);
            frame.render_widget(main_area, chunks[1]);
            frame.render_widget(bottom_bar, chunks[2]);
        })?;

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

                // Toggle wrap/unwrap long lines
                (KeyCode::Char('w'), _) => app.toggle_wrap(),

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
                _ => {
                    eprintln!("{:?} is ignored", key.code)
                }
            }
        }
    }

    // --- TEARDOWN ---
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    Ok(())
}
