mod app;
mod cli;
mod render;
mod xapi_db;
mod xapi_patterns;

use std::fs;
use std::io::stdout;

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use ratatui::{Terminal, backend::CrosstermBackend};

use crate::app::{App, InfoPopup, InfoPopupKind};
use crate::cli::parse_args;
use crate::render::{BOTTOM_BAR_HEIGHT, TOP_BAR_HEIGHT, render};
use crate::xapi_db::Db;
use crate::xapi_patterns::PatternKind;

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
        terminal.draw(|frame| render(frame, &app))?;

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
