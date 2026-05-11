use crate::xapi_db::Db;
use crate::xapi_patterns::{LogLine, PatternKind, first_match_idx, last_match_idx, parse_line};

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;

/// When looking up the DB we can have three different states:
/// 1. DB present and token found,
/// 2. DB present but token not in the DB,
///   - selected object is not an OpaqueRef (so it is expected to not find it)
///   - it is an OpaqueRef (so it is not in the DB)
/// 3. No DB loaded (--db not provided).
pub enum InfoPopupKind {
    Resolved {
        class: String,
        fields: Vec<(String, String)>,
    }, // sorted for display stability
    UnsupportedKind(PatternKind),
    NotInDb,
    NoDb,
}

pub struct InfoPopup {
    pub token: String,
    pub kind: InfoPopupKind,
}

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
pub struct App {
    pub file_path: PathBuf,

    /// The full parsed file. Never mutated after construction. Order matters.
    pub lines: Vec<LogLine>,

    /// Indices into `lines` that pass `active_filters`. Recomputed by `recompute_visible`
    /// whenever filters change.
    pub visible_lines: Vec<usize>,

    /// Index into `visible_lines` of the top visible row. Reset to 0 when filters change.
    pub scroll_offset: usize,

    /// Used to track 'g' pressed twice.
    pub pending_g: bool,

    /// This is the size of the `main_area`.
    pub visible_height: usize,

    /// Currently selected match: `(absolute_line_idx, match_idx_within_line)`.
    /// `None` means nothing selected. Cleared when filters change.
    pub selected: Option<(usize, usize)>,

    /// User-added filter tokens. OR semantics: a line is visible if it contains any of these
    /// substrings.
    pub active_filters: Vec<String>,

    /// When set to true, log lines may span multiple lines. It is false by default.
    pub wrap: bool,

    /// Index of the selected filter when filter panel is opened. None if filter panel is closed.
    pub filter_panel_idx: Option<usize>,

    /// `Some(token)` = info popup is open showing this token.
    /// `None` = closed. Single source of truth for visibility.
    pub info_popup: Option<InfoPopup>,

    /// XAPI Database if it is passed as a parameter
    pub db: Option<Db>,
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
    pub fn new(path: PathBuf, db: Option<Db>) -> io::Result<Self> {
        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        let lines: Vec<LogLine> = reader
            .lines()
            .map_while(Result::ok)
            .map(parse_line)
            .collect();
        // When app starts there are no filters, all lines are visible
        let visible_lines = (0..lines.len()).collect();

        Ok(Self {
            file_path: path,
            lines,
            scroll_offset: 0,
            pending_g: false,
            visible_height: 0, // will be updated each render
            selected: None,
            active_filters: Vec::new(),
            visible_lines,
            wrap: false,
            filter_panel_idx: None,
            info_popup: None,
            db,
        })
    }

    /// Toggles whether long log lines wrap onto multiple terminal rows.
    pub fn toggle_wrap(&mut self) {
        self.wrap = !self.wrap;
    }

    /// Toggles the filter panel visibility. When opened, j/k/Enter operate on
    /// the panel.
    pub fn toggle_filter_panel(&mut self) {
        if self.filter_panel_idx.is_some() {
            self.filter_panel_idx = None;
        } else {
            // TODO: Maybe we should use -1 if the hashset is empty...
            self.filter_panel_idx = Some(0);
        }
    }

    /// Scrolls the filter panel selection up by one, wrapping around if at the top.
    pub fn scroll_filter_panel_idx_up(&mut self) {
        if !self.active_filters.is_empty() {
            self.filter_panel_idx = match self.filter_panel_idx {
                None => None,
                Some(0) => Some(self.active_filters.len() - 1),
                Some(n) => Some(n - 1),
            };
        }
    }

    /// Scrolls the filter panel selection down by one, wrapping around if at the bottom.
    pub fn scroll_filter_panel_idx_down(&mut self) {
        if !self.active_filters.is_empty() {
            self.filter_panel_idx = match self.filter_panel_idx {
                None => None,
                Some(n) if n >= self.active_filters.len() - 1 => Some(0),
                Some(n) => Some(n + 1),
            };
        }
    }

    /// Resets `scroll_offset` to 0, bringing the first visible line to the top
    /// of the viewport.
    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
        eprintln!("set scroll_offset to {}", self.scroll_offset);
    }

    /// Sets `scroll_offset` to the last visible line, bringing the bottom of
    /// the log into view.
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.visible_lines.len().saturating_sub(1);
        eprintln!("set scroll_offset to {}", self.scroll_offset);
    }

    /// Scrolls the viewport up by `n` lines, clamping at the top so that
    /// `scroll_offset` never underflows.
    pub fn scroll_up_by(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
        eprintln!("set scroll_offset to {}", self.scroll_offset);
    }

    /// Scrolls the viewport down by `n` lines, stopping when the last visible
    /// line would scroll off screen.
    pub fn scroll_down_by(&mut self, n: usize) {
        // Don't scroll past the end
        self.scroll_offset =
            (self.scroll_offset + n).min(self.visible_lines.len().saturating_sub(1));
        eprintln!("set scroll_offset to {}", self.scroll_offset);
    }

    /// Adjusts `scroll_offset` so that the currently selected line is within
    /// the viewport.
    ///
    /// If the selection is above the viewport the offset is moved up; if it is
    /// below, the offset is moved down just enough to reveal the last selected
    /// row.  Does nothing when there is no active selection or when the
    /// selected line is not part of `visible_lines`.
    pub fn ensure_selected_visible(&mut self) {
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
    pub fn select(&mut self, sel: (usize, usize)) {
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
    pub fn select_next_match(&mut self, kind: Option<PatternKind>) {
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
    pub fn select_prev_match(&mut self, kind: Option<PatternKind>) {
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
    pub fn clear_selection(&mut self) {
        self.selected = None;
        self.recompute_visible();
    }

    /// Removes all active filter tokens, making every line visible again, and
    /// clears the current selection.
    pub fn clear_filters(&mut self) {
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
    pub fn recompute_visible(&mut self) {
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

        // For debugging purposes, let's print the first 10 values of visible lines.
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

    /// Removes the filter currently highlighted in the filter panel from
    /// `active_filters` and recomputes the visible line set.
    ///
    /// If the removed filter was the last one, the panel index stays at 0
    /// (panel remains open with no filters). Otherwise the index is clamped
    /// to the new last position to keep it in bounds. Does nothing when no
    /// filter is selected or the index is out of range.
    pub fn remove_selected_filter(&mut self) {
        if let Some(idx) = self.filter_panel_idx
            && idx < self.active_filters.len()
        {
            self.active_filters.remove(idx);
            self.recompute_visible();
            // Clamp selection.
            if self.active_filters.is_empty() {
                self.filter_panel_idx = Some(0); // Note: choosing None will close the panel. Not sure what is better.
            } else if idx >= self.active_filters.len() {
                self.filter_panel_idx = Some(self.active_filters.len() - 1);
            }
        }
    }
}
