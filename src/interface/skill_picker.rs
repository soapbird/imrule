//! Searchable multi-select picker for `imrule skills setup`.
//!
//! The state machine and layout are plain data so they can be tested without a
//! terminal; [`run_picker`] is the thin crossterm loop around them.

use std::io::{self, Write};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::{cursor, execute, queue, terminal};

/// One choice in the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerItem {
    /// What `selected_ids` returns for this item — the caller's key, e.g. a
    /// skill's catalog path.
    pub id: String,
    pub title: String,
    /// Dim text after the title (`rust/cli · detected · installed`).
    pub meta: Vec<String>,
    pub description: String,
    pub selected: bool,
}

/// Keys the picker understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKey {
    Char(char),
    Backspace,
    Up,
    Down,
    PageUp,
    PageDown,
    Toggle,
    ToggleAll,
    Confirm,
    Cancel,
}

/// What the loop should do after a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerAction {
    Continue,
    Confirm,
    Cancel,
}

/// How a rendered span is styled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Normal,
    Strong,
    Dim,
    Accent,
    Highlight,
}

/// A styled piece of a rendered line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub tone: Tone,
}

fn span(text: impl Into<String>, tone: Tone) -> Span {
    Span {
        text: text.into(),
        tone,
    }
}

/// Rows each item occupies: title, description, spacer.
const ITEM_ROWS: usize = 3;
/// Rows above the list: title, detection, search box (3), spacer.
const HEADER_ROWS: usize = 6;
/// Rows below the list: "more below", hints.
const FOOTER_ROWS: usize = 2;

/// Picker state.
#[derive(Debug, Clone)]
pub struct Picker {
    title: String,
    subtitle: String,
    items: Vec<PickerItem>,
    query: String,
    /// Index into the filtered list.
    cursor: usize,
    /// First filtered index shown.
    offset: usize,
}

impl Picker {
    pub fn new(
        title: impl Into<String>,
        subtitle: impl Into<String>,
        items: Vec<PickerItem>,
    ) -> Self {
        Self {
            title: title.into(),
            subtitle: subtitle.into(),
            items,
            query: String::new(),
            cursor: 0,
            offset: 0,
        }
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// Indices of items whose title, meta, or description contain every
    /// whitespace-separated term of the query, case-insensitively. Title
    /// matches come first, then meta, then description-only matches — so
    /// typing a skill's name focuses that skill even when other descriptions
    /// mention it. Item order is kept within each rank.
    pub fn filtered(&self) -> Vec<usize> {
        let terms: Vec<String> = self
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let all_in = |text: &str| {
            let text = text.to_lowercase();
            terms.iter().all(|term| text.contains(term))
        };
        let mut ranked: Vec<(u8, usize)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let meta = item.meta.join(" ");
                let rank = if all_in(&item.title) {
                    0
                } else if all_in(&format!("{} {meta}", item.title)) {
                    1
                } else if all_in(&format!("{} {meta} {}", item.title, item.description)) {
                    2
                } else {
                    return None;
                };
                Some((rank, index))
            })
            .collect();
        ranked.sort();
        ranked.into_iter().map(|(_, index)| index).collect()
    }

    /// Indices of the selected items, in item order.
    pub fn selected(&self) -> Vec<usize> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.selected)
            .map(|(index, _)| index)
            .collect()
    }

    /// Ids of the selected items, in item order.
    pub fn selected_ids(&self) -> Vec<String> {
        self.items
            .iter()
            .filter(|item| item.selected)
            .map(|item| item.id.clone())
            .collect()
    }

    pub fn handle(&mut self, key: PickerKey, list_height: usize) -> PickerAction {
        let visible = self.filtered();
        let page = (list_height / ITEM_ROWS).max(1);
        match key {
            PickerKey::Char(c) => {
                self.query.push(c);
                self.cursor = 0;
                self.offset = 0;
            }
            PickerKey::Backspace => {
                self.query.pop();
                self.cursor = 0;
                self.offset = 0;
            }
            PickerKey::Up => self.cursor = self.cursor.saturating_sub(1),
            PickerKey::Down => {
                if self.cursor + 1 < visible.len() {
                    self.cursor += 1;
                }
            }
            PickerKey::PageUp => self.cursor = self.cursor.saturating_sub(page),
            PickerKey::PageDown => {
                self.cursor = (self.cursor + page).min(visible.len().saturating_sub(1));
            }
            PickerKey::Toggle => {
                if let Some(&index) = visible.get(self.cursor) {
                    self.items[index].selected = !self.items[index].selected;
                }
            }
            PickerKey::ToggleAll => {
                let select = visible.iter().any(|&index| !self.items[index].selected);
                for index in visible {
                    self.items[index].selected = select;
                }
            }
            PickerKey::Confirm => return PickerAction::Confirm,
            PickerKey::Cancel => return PickerAction::Cancel,
        }
        self.scroll_into_view(page);
        PickerAction::Continue
    }

    fn scroll_into_view(&mut self, page: usize) {
        if self.cursor < self.offset {
            self.offset = self.cursor;
        } else if self.cursor >= self.offset + page {
            self.offset = self.cursor + 1 - page;
        }
    }

    /// Height available to the list for a terminal of `height` rows.
    pub fn list_height(height: usize) -> usize {
        height
            .saturating_sub(HEADER_ROWS + FOOTER_ROWS)
            .max(ITEM_ROWS)
    }

    /// Lays the picker out for a `width` × `height` terminal.
    pub fn render(&self, width: usize, height: usize) -> Vec<Vec<Span>> {
        let width = width.max(20);
        let visible = self.filtered();
        let mut lines = Vec::new();

        lines.push(vec![
            span(format!(" {}", self.title), Tone::Strong),
            span(
                format!(
                    "  ({} selected · {}/{})",
                    self.selected().len(),
                    visible.len(),
                    self.items.len()
                ),
                Tone::Dim,
            ),
        ]);
        lines.push(vec![span(format!(" {}", self.subtitle), Tone::Dim)]);

        let inner = width.saturating_sub(4);
        lines.push(vec![span(format!(" ╭{}╮", "─".repeat(inner)), Tone::Dim)]);
        let (text, tone) = if self.query.is_empty() {
            ("⌕ Search…".to_string(), Tone::Dim)
        } else {
            (format!("⌕ {}▏", self.query), Tone::Normal)
        };
        let text = truncate(&text, inner.saturating_sub(2));
        let padding = inner.saturating_sub(2 + display_width(&text));
        lines.push(vec![
            span(" │ ", Tone::Dim),
            span(text, tone),
            span(format!("{} │", " ".repeat(padding)), Tone::Dim),
        ]);
        lines.push(vec![span(format!(" ╰{}╯", "─".repeat(inner)), Tone::Dim)]);
        lines.push(Vec::new());

        let page = (Self::list_height(height) / ITEM_ROWS).max(1);
        if visible.is_empty() {
            lines.push(vec![span("   No skills match your search.", Tone::Dim)]);
        }
        for (position, &index) in visible.iter().enumerate().skip(self.offset).take(page) {
            let item = &self.items[index];
            let focused = position == self.cursor;
            let mut title = vec![
                span(if focused { " ❯ " } else { "   " }, Tone::Accent),
                span(
                    if item.selected { "● " } else { "○ " },
                    if item.selected {
                        Tone::Accent
                    } else {
                        Tone::Normal
                    },
                ),
                span(
                    item.title.clone(),
                    if focused {
                        Tone::Highlight
                    } else {
                        Tone::Strong
                    },
                ),
            ];
            for meta in &item.meta {
                title.push(span(format!(" · {meta}"), Tone::Dim));
            }
            lines.push(title);
            lines.push(vec![span(
                format!(
                    "     {}",
                    truncate(&item.description, width.saturating_sub(6))
                ),
                Tone::Dim,
            )]);
            lines.push(Vec::new());
        }

        let remaining = visible.len().saturating_sub(self.offset + page);
        lines.push(if remaining > 0 {
            vec![span(format!("  ↓ {remaining} more below"), Tone::Dim)]
        } else {
            Vec::new()
        });
        lines.push(vec![span(
            "  Type to search · Space to toggle · Ctrl+A toggle all · Enter to install · Esc to cancel",
            Tone::Dim,
        )]);
        lines
    }
}

/// Maps a crossterm key event onto a picker key.
pub fn picker_key(event: KeyEvent) -> Option<PickerKey> {
    if event.kind != KeyEventKind::Press {
        return None;
    }
    let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
    Some(match event.code {
        KeyCode::Char('c') if ctrl => PickerKey::Cancel,
        KeyCode::Char('a') if ctrl => PickerKey::ToggleAll,
        KeyCode::Char('p') if ctrl => PickerKey::Up,
        KeyCode::Char('n') if ctrl => PickerKey::Down,
        KeyCode::Char(' ') | KeyCode::Tab => PickerKey::Toggle,
        KeyCode::Char(c) if !ctrl => PickerKey::Char(c),
        KeyCode::Backspace => PickerKey::Backspace,
        KeyCode::Up => PickerKey::Up,
        KeyCode::Down => PickerKey::Down,
        KeyCode::PageUp => PickerKey::PageUp,
        KeyCode::PageDown => PickerKey::PageDown,
        KeyCode::Enter => PickerKey::Confirm,
        KeyCode::Esc => PickerKey::Cancel,
        _ => return None,
    })
}

/// Restores the terminal however the picker exits.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stderr(), cursor::Show, terminal::LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

/// Runs the picker on stderr's terminal. Returns the selected items' ids, or
/// `None` when the user cancels.
pub fn run_picker(mut picker: Picker) -> io::Result<Option<Vec<String>>> {
    terminal::enable_raw_mode()?;
    let _guard = TerminalGuard;
    let mut out = io::stderr();
    execute!(out, terminal::EnterAlternateScreen, cursor::Hide)?;

    loop {
        let (columns, rows) = terminal::size()?;
        draw(
            &mut out,
            &picker.render(columns as usize, rows as usize),
            rows as usize,
        )?;
        // Any other event (a resize, focus, mouse) just redraws.
        let Event::Key(key) = event::read()? else {
            continue;
        };
        let Some(key) = picker_key(key) else {
            continue;
        };
        match picker.handle(key, Picker::list_height(rows as usize)) {
            PickerAction::Continue => {}
            PickerAction::Confirm => return Ok(Some(picker.selected_ids())),
            PickerAction::Cancel => return Ok(None),
        }
    }
}

fn draw(out: &mut impl Write, lines: &[Vec<Span>], rows: usize) -> io::Result<()> {
    queue!(
        out,
        cursor::MoveTo(0, 0),
        terminal::Clear(terminal::ClearType::All)
    )?;
    for (row, line) in lines.iter().take(rows).enumerate() {
        queue!(out, cursor::MoveTo(0, row as u16))?;
        for piece in line {
            match piece.tone {
                Tone::Normal => {}
                Tone::Strong => queue!(out, SetAttribute(Attribute::Bold))?,
                Tone::Dim => queue!(out, SetForegroundColor(Color::DarkGrey))?,
                Tone::Accent => queue!(out, SetForegroundColor(Color::Magenta))?,
                Tone::Highlight => queue!(
                    out,
                    SetAttribute(Attribute::Bold),
                    SetForegroundColor(Color::Magenta)
                )?,
            }
            queue!(
                out,
                Print(&piece.text),
                SetAttribute(Attribute::Reset),
                ResetColor
            )?;
        }
    }
    out.flush()
}

/// Terminal columns a string occupies, counting East Asian wide characters
/// (Hangul, CJK, full-width forms, emoji) as two.
pub fn display_width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

fn char_width(c: char) -> usize {
    let code = c as u32;
    let wide = matches!(code,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD);
    if wide { 2 } else { 1 }
}

/// Cuts `text` to at most `max` columns, ending in `…` when shortened.
/// Whitespace runs are collapsed to single spaces first, so multi-line or
/// indented text still renders on one line.
pub fn truncate(text: &str, max: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if display_width(&text) <= max {
        return text;
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = char_width(c);
        if used + w + 1 > max {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}
