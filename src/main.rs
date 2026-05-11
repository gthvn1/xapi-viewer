mod app;
mod cli;
mod xapi_db;
mod xapi_patterns;

use std::{borrow::Cow, fs, io::stdout};

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::app::{App, InfoPopup, InfoPopupKind};
use crate::cli::parse_args;
use crate::xapi_db::Db;
use crate::xapi_patterns::{LogLine, PatternKind};

const TOP_BAR_HEIGHT: u16 = 1;
const BOTTOM_BAR_HEIGHT: u16 = 2;

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
/// they exceed `line_size` columns.
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
    }

    ListItem::new(Text::from(rows))
}

/// Returns a [`Rect`] centred within `area`, sized as a percentage of it.
///
/// `percent_x` and `percent_y` are the desired width and height as
/// percentages of `area` (0–100). The remaining space is split equally on
/// both sides to produce a centred popup region.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    use ratatui::layout::{Constraint, Direction, Layout};

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Entry point.
///
/// Reads the log file path from the first command-line argument, loads it
/// into an [`App`], then enters the ratatui TUI event loop.  The loop redraws
/// the screen on every iteration and blocks on a key event before updating
/// state.  The terminal is restored to its original state when the user quits
/// with `q`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse args: read path from command line
    let argv = std::env::args().collect::<Vec<String>>();
    let args = match parse_args(&argv) {
        Err(e) => {
            eprintln!("{}", e);
            eprintln!("Usage: xapi-viewer <path>");
            std::process::exit(1);
        }
        Ok(a) => a,
    };

    eprintln!("log file: {:?}", args.log_file);
    eprintln!("db file: {:?}", args.db_file);

    let db: Option<Db> = if let Some(db_file) = args.db_file {
        let t0 = std::time::Instant::now();
        let db_string: String = fs::read_to_string(&db_file)?;
        let read_elapsed = t0.elapsed();

        let t1 = std::time::Instant::now();
        let db = Db::parse(&db_string)?;
        let parse_elapsed = t1.elapsed();
        eprintln!(
            "Loaded {:?}: read {:?}, parse {:?}",
            db_file, read_elapsed, parse_elapsed
        );
        Some(db)
    } else {
        None
    };

    // Load file before entering TUI mode
    let mut app = App::new(args.log_file, db)?;

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
                    app.file_path.display(),
                    app.scroll_offset + 1,
                    app.lines.len())
            } else {
                format!(
                    "xapi-viewer - {} ({}/{}) filtered, {} active",
                    app.file_path.display(),
                    app.scroll_offset + 1,
                    app.visible_lines.len(),
                    app.active_filters.len())
            };

            // TOP
            let top_bar = Paragraph::new(top_text).style(bar_style);

            // BOTTOM
            let bottom_bar =
                Paragraph::new(
                    "q=quit  j/k=scroll  Ctrl-j/k=half  PgUp/Dn=page  gg/G=top/bot  w=toggle-long-lines  f=toggle-filter-panel\n\
                    Tab/S-Tab=match  d/r/t/u/o=next-kind  D/R/T/U/O=prev-kind  Enter=filter  x=clear-filters  i=info  Esc=unsel")
                .style(bar_style);

            // MIDDLE AREA: either logs or logs + filters
            let middle = chunks[1];
            let (log_area, filter_area) = if app.filter_panel_idx.is_some() {
                let h_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
                    .split(middle);
                (h_chunks[0], Some(h_chunks[1]))
            } else {
                (middle, None)
            };

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
                    render_log_line(log_line, selected_match_idx, abs_idx, log_area.width as usize, app.wrap)
                })
            .collect();

            frame.render_widget(top_bar, chunks[0]);
            frame.render_widget(List::new(items), log_area);

            if let Some(panel) = filter_area {
                let panel_block = Block::default().borders(Borders::ALL).title("Filters");
                if app.active_filters.is_empty() {
                    frame.render_widget(Paragraph::new("No active filter").block(panel_block),panel);
                } else {
                    let filters: Vec<ListItem> = app
                        .active_filters
                        .iter()
                        .enumerate()
                        .map(|(i, s)| {
                            let style = if Some(i) == app.filter_panel_idx {
                                Style::default().add_modifier(Modifier::REVERSED)
                            } else {
                                Style::default()
                            };
                            ListItem::new(Span::styled(s.as_str(), style))})
                        .collect();
                    frame.render_widget(List::new(filters).block(panel_block),panel);
                }
            };
            frame.render_widget(bottom_bar, chunks[2]);

            if let Some(popup) = &app.info_popup {
                let area = centered_rect(60, 30, frame.area());
                let block = Block::default()
                    .title(" Info ")
                    .borders(Borders::ALL);

                let mut lines = vec![Line::from(popup.token.as_str()), Line::from("")];

                match &popup.kind {
                    InfoPopupKind::Resolved {class, fields } =>{
                        lines.push(Line::from(format!("Class: {}", class)));
                        lines.push(Line::from(""));
                        for (k,v) in fields {
                            lines.push(Line::from(format!("{} : {}", k, v)));
                        }
                    }
                    InfoPopupKind::NotInDb => {
                        lines.push(Line::from("Not found in database."));
                    }
                    InfoPopupKind::NoDb =>  {
                        lines.push(Line::from("No database loaded (use --db to load XAPI db)."));
                    }
                    InfoPopupKind::UnsupportedKind(k) => {
                        lines.push(Line::from(format!("info lookup not supported for {:?}.", k)));
                    }
                }

                lines.push(Line::from(""));
                lines.push(Line::from("Esc to close")
                    .style(Style::default().fg(Color::DarkGray)));

                let paragraph = Paragraph::new(lines).block(block);
                frame.render_widget(Clear, area);   // wipe what's beneath
                frame.render_widget(paragraph, area);
            }

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

            // Highest precedence is info popup. Once opened we are waiting to close it
            // by either pressing Esc or 'i'.
            if app.info_popup.is_some() {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('i') => app.info_popup = None,
                    _ => {}
                }
                continue;
            }

            // Next by precedence is filter panel.
            if app.filter_panel_idx.is_some() {
                match key.code {
                    // NOTE: don't quit when filter panel is open
                    // Toggle filter panel
                    KeyCode::Char('f') => app.toggle_filter_panel(),
                    KeyCode::Char('k') | KeyCode::Up => app.scroll_filter_panel_idx_up(),
                    KeyCode::Char('j') | KeyCode::Down => app.scroll_filter_panel_idx_down(),
                    KeyCode::Enter => app.remove_selected_filter(),
                    _ => {
                        eprintln!("{:?} is ignored", key.code)
                    }
                }
                continue;
            }

            // And the default: log view mode.
            match key.code {
                KeyCode::Char('q') => break,

                // clear filters
                KeyCode::Char('x') => app.clear_filters(),

                // Scroll down (half_page, one line, page)
                KeyCode::Char('j') | KeyCode::Down => {
                    let n = if key.modifiers.contains(KeyModifiers::CONTROL) {
                        half_page
                    } else {
                        1
                    };
                    app.scroll_down_by(n);
                }
                KeyCode::PageDown => app.scroll_down_by(full_page),

                // Scroll up (half_page, one line, page)
                KeyCode::Char('k') | KeyCode::Up => {
                    let n = if key.modifiers.contains(KeyModifiers::CONTROL) {
                        half_page
                    } else {
                        1
                    };
                    app.scroll_up_by(n);
                }
                KeyCode::PageUp => app.scroll_up_by(full_page),

                // Scroll top and bottom
                KeyCode::Home => app.scroll_to_top(),
                KeyCode::Char('G') | KeyCode::End => app.scroll_to_bottom(),

                // Select/Unselect matches
                KeyCode::Esc => app.clear_selection(),
                KeyCode::Tab => app.select_next_match(None),
                KeyCode::BackTab => app.select_prev_match(None),
                KeyCode::Char('d') => app.select_next_match(Some(PatternKind::TaskId)),
                KeyCode::Char('r') => app.select_next_match(Some(PatternKind::RequestId)),
                KeyCode::Char('t') => app.select_next_match(Some(PatternKind::TrackId)),
                KeyCode::Char('u') => app.select_next_match(Some(PatternKind::Uuid)),
                KeyCode::Char('o') => app.select_next_match(Some(PatternKind::OpaqueRef)),
                // As we are using SHIFT to go backward, characters will be uppercase...
                KeyCode::Char('D') => app.select_prev_match(Some(PatternKind::TaskId)),
                KeyCode::Char('R') => app.select_prev_match(Some(PatternKind::RequestId)),
                KeyCode::Char('T') => app.select_prev_match(Some(PatternKind::TrackId)),
                KeyCode::Char('U') => app.select_prev_match(Some(PatternKind::Uuid)),
                KeyCode::Char('O') => app.select_prev_match(Some(PatternKind::OpaqueRef)),

                // Toggle wrap/unwrap long lines
                KeyCode::Char('w') => app.toggle_wrap(),

                // Toggle info popup
                KeyCode::Char('i') => {
                    if let Some((line_idx, match_idx)) = app.selected {
                        let line = &app.lines[line_idx];
                        let m = &line.matches[match_idx];
                        let token = line.raw[m.range.clone()].to_string();

                        let kind = match m.kind {
                            PatternKind::OpaqueRef => match &app.db {
                                None => InfoPopupKind::NoDb,
                                Some(db) => match db.get(&token) {
                                    None => InfoPopupKind::NotInDb,
                                    Some(obj) => {
                                        let mut fields: Vec<_> = obj
                                            .fields
                                            .iter()
                                            .map(|(k, v)| (k.clone(), v.clone()))
                                            .collect();
                                        fields.sort_by(|a, b| a.0.cmp(&b.0));
                                        InfoPopupKind::Resolved {
                                            class: obj.class.clone(),
                                            fields,
                                        }
                                    }
                                },
                            },
                            other => InfoPopupKind::UnsupportedKind(other),
                        };

                        app.info_popup = Some(InfoPopup { token, kind });
                    }
                    // If nothing is selected, silently do nothing.
                }

                // Toggle filter panel
                KeyCode::Char('f') => app.toggle_filter_panel(),

                // Toggle match in active filters
                KeyCode::Enter => {
                    if let Some((line_idx, match_idx)) = app.selected {
                        let log_line = &app.lines[line_idx];
                        let m = &log_line.matches[match_idx];
                        let token = log_line.raw[m.range.clone()].to_string();

                        if app.active_filters.contains(&token) {
                            eprintln!("{} removed from active filters", &token);
                            app.active_filters.retain(|f| f != &token);
                        } else {
                            eprintln!("{} added in active filters", &token);
                            app.active_filters.push(token);
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
