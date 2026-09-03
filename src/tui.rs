use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, Paragraph},
    backend::CrosstermBackend,
    Terminal,
};
use std::io;

use crate::app::{App, AppState};

use unicode_width::UnicodeWidthStr;

// ticker: typing animation per 1.7 detik
const NAMES: &[&str] = &["dhodo", "zan", "ian", "firman", "gilang", "kurawal", "reyner", "fakhri"];
const CYCLE_MS: u128 = 1700;
const CHARS_PER_MS: f64 = 3.0; // 3 frame per char @ 50ms/frame

/// Returns (display_text, show_cursor)
fn ticker_typing(ticker: std::time::Instant) -> (String, bool) {
    let ms = ticker.elapsed().as_millis();
    let idx = (ms / CYCLE_MS) as usize % NAMES.len();
    let name = NAMES[idx];
    let phase = ms % CYCLE_MS;
    let typing_ms = (name.len() as f64 / CHARS_PER_MS * 50.0) as u128;
    if phase < typing_ms {
        // typing in progress
        let chars = (phase as f64 / 50.0 * CHARS_PER_MS).ceil() as usize;
        let visible = chars.min(name.len());
        (name[..visible].to_string(), true)
    } else {
        // pause: show full name + blinking cursor
        let blink = ((phase - typing_ms) / 300).is_multiple_of(2);
        (name.to_string(), blink)
    }
}

pub const MENU_OPTIONS: [&str; 6] = [
    " Commit ",
    " Edit  ",
    " Retry ",
    " Lang  ",
    "Manual ",
    " Quit  ",
];

const MSG_W: u16 = 64;
const PAD_LEFT: u16 = 2;

fn msg_width(terminal_w: u16) -> u16 {
    (terminal_w * 90 / 100).clamp(48, MSG_W)
}

fn msg_lines(msg: &str, w: u16) -> u16 {
    if w == 0 || msg.is_empty() {
        return 1;
    }
    let cw = w as usize;
    let mut lines = 0u16;
    for paragraph in msg.split('\n') {
        if paragraph.is_empty() {
            lines += 1;
        } else {
            lines += paragraph.len().div_ceil(cw) as u16;
        }
    }
    lines.max(1)
}

// rumus: potong + ganjal blank sampe pas `w` cell (display-width, bukan byte/chars)
fn fit_width(s: &str, w: usize) -> String {
    let mut out = String::new();
    let mut cw = 0;
    for c in s.chars() {
        let chw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if cw + chw > w {
            break;
        }
        out.push(c);
        cw += chw;
    }
    while cw < w {
        out.push(' ');
        cw += 1;
    }
    out
}

pub fn render<B: io::Write>(
    terminal: &mut Terminal<CrosstermBackend<B>>,
    app: &mut App,
    frame: u32,
) -> io::Result<()> {
    terminal.draw(|f| {
        let area = f.size();

        match &app.state {
            AppState::Quit => {
                let box_w = msg_width(area.width).min(area.width.saturating_sub(PAD_LEFT));
                render_message(f, Rect { x: area.x + PAD_LEFT, y: area.y, width: box_w, height: area.height }, &app.commit_msg);
            }
            AppState::Loading(txt) => {
                let box_w = msg_width(area.width);
                let content_w = box_w.saturating_sub(2);
                let msg_h = msg_lines(&app.commit_msg, content_w) + 2;
                // margin atas 1 + header(2) + msg box
                render_header(f, Rect { x: area.x + PAD_LEFT, y: area.y + 1, width: box_w.min(area.width.saturating_sub(PAD_LEFT)), height: 2 }, app);
                let msg_area = Rect { x: area.x + PAD_LEFT, y: area.y + 3, width: box_w, height: msg_h };
                render_message(f, msg_area, &app.commit_msg);
                // ponytail: overlay tepat menutupi garis └─┘
                render_loading(f, msg_area, txt, frame);
            }
            _ => {
                let box_w = msg_width(area.width);
                let content_w = box_w.saturating_sub(2);
                let msg_h = msg_lines(&app.commit_msg, content_w) + 2;

                let pad = Rect {
                    x: area.x + PAD_LEFT,
                    y: area.y,
                    width: box_w.min(area.width.saturating_sub(PAD_LEFT)),
                    height: area.height,
                };
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(1),
                        Constraint::Length(2),
                        Constraint::Length(msg_h),
                        Constraint::Length(1),
                        Constraint::Length(2),
                    ])
                    .split(pad);

                render_header(f, chunks[1], app);
                render_message(f, chunks[2], &app.commit_msg);
                render_menu(f, chunks[4], app.selected_option);
                render_overlay(f, app);
            }
        }
    })?;
    Ok(())
}

fn render_header(f: &mut ratatui::Frame, area: Rect, app: &App) {
    use ratatui::text::{Line, Span};
    let lang = if app.is_indo { "ID" } else { "EN" };
    let short = app.model.rsplit('/').next().unwrap_or(&app.model);
    let (display, _show_cursor) = ticker_typing(app.ticker);
    let name_text = display.to_uppercase();
    let prefix = " 🐰 COMMIT LINT GEN BY ";
    let model_text = format!(" ·  {}", short.to_uppercase());
    let w = area.width as usize;
    let max_name = NAMES.iter().map(|n| n.len()).max().unwrap_or(0);
    let model_end = prefix.width() + max_name + model_text.width();
    let gap_to_lang = w.saturating_sub(model_end + lang.width());
    let spans = vec![
        Span::styled(prefix, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(name_text.clone(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled(format!("{}{}", " ".repeat(max_name - name_text.len()), model_text), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(" ".repeat(gap_to_lang), Style::default()),
        Span::styled(lang, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ];
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_message(f: &mut ratatui::Frame, area: Rect, msg: &str) {
    if area.height < 3 || area.width < 10 {
        return;
    }

    // lebar = lebar area dari caller (sudah box_w), jangan hitung ulang
    let w = area.width;
    let cw = w.saturating_sub(2) as usize;
    let ch = area.height.saturating_sub(2) as usize;

    // ─── (tanpa pojok)
    let top = "─".repeat(w as usize);
    f.render_widget(
        Paragraph::new(top).style(Style::default().fg(Color::DarkGray)),
        Rect { x: area.x, y: area.y, width: w, height: 1 },
    );

    // message content — manual wrap
    let mut lines: Vec<ratatui::text::Line> = Vec::new();
    for paragraph in msg.split('\n') {
        if paragraph.is_empty() {
            lines.push(ratatui::text::Line::from(""));
            continue;
        }
        let mut rest = paragraph;
        while !rest.is_empty() {
            let take = cw.min(rest.len());
            lines.push(ratatui::text::Line::from(&rest[..take]));
            rest = &rest[take..];
        }
    }
    lines.truncate(ch);

    f.render_widget(
        Paragraph::new(lines).style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Rect { x: area.x + 1, y: area.y + 1, width: cw as u16, height: ch as u16 },
    );

    // ─── (tanpa pojok)
    let bot = "─".repeat(w as usize);
    f.render_widget(
        Paragraph::new(bot).style(Style::default().fg(Color::DarkGray)),
        Rect { x: area.x, y: area.y + 1 + ch as u16, width: w, height: 1 },
    );
}

// rumus justify: total label + gap dibagi rata = pas w cell; sisa motong via fit_width
fn justified_line(items: &[String], w: usize) -> String {
    let total: usize = items.iter().map(|s| s.as_str().width()).sum();
    let gaps = items.len().saturating_sub(1);
    if gaps == 0 {
        return fit_width(&items.join(""), w);
    }
    let (base, mut rem) = if total < w { ((w - total) / gaps, (w - total) % gaps) } else { (1, 0) };
    let mut out = String::new();
    for (i, s) in items.iter().enumerate() {
        out.push_str(s);
        if i < gaps {
            let g = base + if rem > 0 { rem -= 1; 1 } else { 0 };
            out.push_str(&" ".repeat(g));
        }
    }
    fit_width(&out, w)
}

fn render_menu(f: &mut ratatui::Frame, area: Rect, selected: usize) {
    use ratatui::text::{Line, Span};
    let w = area.width as usize;
    let labels: Vec<String> = MENU_OPTIONS
        .iter()
        .enumerate()
        .map(|(i, opt)| format!("{}{}", if i == selected { "▸ " } else { "  " }, opt.trim()))
        .collect();
    // warna per item: bangun ulang dari justified string dengan span per label
    let line = justified_line(&labels, w);
    let mut spans = Vec::new();
    let mut rest = line.as_str();
    for (i, label) in labels.iter().enumerate() {
        let fg = if i == selected { Color::Yellow } else { Color::White };
        let style = Style::default().fg(fg).add_modifier(Modifier::BOLD);
        if let Some(pos) = rest.find(label.as_str()) {
            if pos > 0 {
                spans.push(Span::raw(rest[..pos].to_string()));
            }
            spans.push(Span::styled(label.clone(), style));
            rest = &rest[pos + label.len()..];
        }
    }
    if !rest.is_empty() {
        spans.push(Span::raw(rest.to_string()));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_loading(f: &mut ratatui::Frame, msg_area: Rect, txt: &str, frame: u32) {
    use ratatui::layout::Alignment;
    let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let icon = spinner[(frame as usize) % 10];

    // ponytail: single spinner, selebar msg box, bottom pas di garis └─┘
    let h = 3;
    let box_area = Rect { x: msg_area.x, y: msg_area.y + msg_area.height.saturating_sub(h), width: msg_area.width, height: h };

    f.render_widget(Clear, box_area);
    f.render_widget(
        Paragraph::new(format!("{} {}", icon, txt))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .style(Style::default().fg(Color::Cyan)),
        box_area,
    );
}

fn render_overlay(f: &mut ratatui::Frame, app: &App) {
    if matches!(app.state, AppState::Editing | AppState::Manual) {
        let area = centered(70, 30, f.size());
        // ponytail: efek blur = redupkan semua cell di luar overlay
        let full = f.size();
        let buf = f.buffer_mut();
        for y in full.y..full.y + full.height {
            for x in full.x..full.x + full.width {
                if x < area.x || x >= area.x + area.width || y < area.y || y >= area.y + area.height {
                    buf.get_mut(x, y).modifier |= Modifier::DIM;
                }
            }
        }
        f.render_widget(Clear, area);
        let title = if matches!(app.state, AppState::Editing) {
            " Edit Message "
        } else {
            " Manual Commit "
        };
        let widget = Paragraph::new(app.edit_input.as_str())
            .style(Style::default().fg(Color::Yellow))
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow))
                    .padding(ratatui::widgets::Padding::uniform(1)),
            );
        f.render_widget(widget, area);
        // ponytail: cursor nyata di posisi edit_cursor (padding border 1 + pad 1 = +2)
        let before: String = app.edit_input.chars().take(app.edit_cursor).collect();
        let row = before.chars().filter(|&c| c == '\n').count() as u16;
        let last = before.rsplit('\n').next().unwrap_or("");
        use unicode_width::UnicodeWidthStr;
        f.set_cursor(area.x + 2 + last.width() as u16, area.y + 2 + row);
    }
}

fn centered(px: u16, py: u16, r: Rect) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - py) / 2),
            Constraint::Percentage(py),
            Constraint::Percentage((100 - py) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - px) / 2),
            Constraint::Percentage(px),
            Constraint::Percentage((100 - px) / 2),
        ])
        .split(v[1])[1]
}
