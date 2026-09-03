mod ai;
mod app;
mod git;
mod tui;
mod types;

use app::{App, AppState};
use crossterm::{cursor, execute};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;
use std::time::Duration;

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|s| s.as_str()) == Some("config") {
        return setup_config();
    } else if args.len() > 1 {
        eprintln!("Pakai: clg [config]");
        return Ok(());
    }
    let cfg = types::Config::load();
    if cfg.apiKey.trim().is_empty() {
        eprintln!("Error: apiKey kosong di ~/.commitlintgenrc.json.");
        return Ok(());
    }
    let diff = match git::staged_diff() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error: {}", e);
            return Ok(());
        }
    };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(&mut stdout);
    let mut terminal = Terminal::new(backend)?;

    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    let mut app = App {
        commit_msg: String::new(),
        edit_input: String::new(),
        edit_cursor: 0,
        model: cfg.model.clone(),
        base_url: cfg.baseURL.clone(),
        api_key: cfg.apiKey.clone(),
        is_indo: cfg.language.to_lowercase().starts_with("id"),
        selected_option: 0,
        state: AppState::Loading("Membuat pesan commit...".into()),
        diff,
        pending: None,
        rt,
        ticker: std::time::Instant::now(),
    };

    spawn_generate(&mut app);

    let mut frame: u32 = 0;
    let mut last_ctrlc: Option<std::time::Instant> = None;
    loop {
        frame = frame.wrapping_add(1);

        // Check if AI result arrived
        if let Some(rx) = &app.pending
            && let Ok(result) = rx.try_recv()
        {
            app.pending = None;
            match result {
                Ok(msg) => {
                    app.commit_msg = msg;
                    app.state = AppState::Menu;
                }
                Err(e) => {
                    cleanup(&mut terminal)?;
                    eprintln!("API Error: {}", e);
                    return Ok(());
                }
            }
        }

        tui::render(&mut terminal, &mut app, frame)?;

        if matches!(app.state, AppState::Quit) {
            cleanup(&mut terminal)?;
            // ponytail: sisakan commit msg berborder, margin 0
            print_leftover(&app.commit_msg);
            break Ok(());
        }

        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            // ponytail: Ctrl+C 2x (jeda <2s) baru kill
            if key.code == KeyCode::Char('c') && key.modifiers.contains(event::KeyModifiers::CONTROL) {
                let now = std::time::Instant::now();
                if last_ctrlc.is_some_and(|t| now.duration_since(t).as_secs() < 2) {
                    cleanup(&mut terminal)?;
                    return Ok(());
                }
                last_ctrlc = Some(now);
                continue;
            }
            match &app.state {
                AppState::Menu => handle_menu(&key.code, &mut app),
                AppState::Editing | AppState::Manual => handle_input(&key.code, &mut app)?,
                _ => {}
            }
        }
    }
}

fn setup_config() -> io::Result<()> {
    use std::io::Write;
    let mut cfg = types::Config::load();
    let ask = |label: &str, cur: &str| -> io::Result<String> {
        if cur.is_empty() {
            print!("{}: ", label);
        } else {
            print!("{} [{}]: ", label, cur);
        }
        io::stdout().flush()?;
        let mut s = String::new();
        io::stdin().read_line(&mut s)?;
        let s = s.trim().to_string();
        Ok(if s.is_empty() { cur.to_string() } else { s })
    };
    // ponytail: key lama disensor, input kosong = pakai yang lama
    let masked = if cfg.apiKey.len() > 14 {
        format!("{}...{}", &cfg.apiKey[..8], &cfg.apiKey[cfg.apiKey.len() - 8..])
    } else if cfg.apiKey.is_empty() {
        String::new()
    } else {
        "********".to_string()
    };
    cfg.apiKey = ask("Kunci API Groq", &masked).map(|s| if s == masked { cfg.apiKey.clone() } else { s })?;
    cfg.model = ask("Model", &cfg.model)?;
    if cfg.apiKey.trim().is_empty() {
        eprintln!("Error: apiKey wajib diisi.");
        return Ok(());
    }
    let path = types::Config::path();
    std::fs::write(&path, serde_json::to_string_pretty(&cfg).unwrap())?;
    println!("Tersimpan di {}", path);
    Ok(())
}

fn spawn_generate(app: &mut App) {
    let (tx, rx) = std::sync::mpsc::channel();
    let diff = app.diff.clone();
    let key = app.api_key.clone();
    let base = app.base_url.clone();
    let model = app.model.clone();
    let is_indo = app.is_indo;
    app.pending = Some(rx);

    app.rt.spawn(async move {
        let result = ai::generate_commit(&diff, &key, &base, &model, is_indo).await;
        let _ = tx.send(result);
    });
}

fn handle_menu(key: &KeyCode, app: &mut App) {
    match key {
        KeyCode::Left | KeyCode::Char('h') => {
            if app.selected_option > 0 {
                app.selected_option -= 1;
            }
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if app.selected_option < tui::MENU_OPTIONS.len() - 1 {
                app.selected_option += 1;
            }
        }
        KeyCode::Enter => match app.selected_option {
            0 => {
                if let Err(e) = git::commit(&app.commit_msg) {
                    eprintln!("Error: {}", e);
                } else {
                    println!("✅ Commit berhasil!");
                }
                app.state = AppState::Quit;
            }
            1 => {
                app.edit_input = app.commit_msg.clone();
                app.edit_cursor = app.edit_input.chars().count();
                app.state = AppState::Editing;
            }
            2 => {
                app.state = AppState::Loading("Membuat ulang...".into());
                spawn_generate(app);
            }
            3 => {
                app.is_indo = !app.is_indo;
                // ponytail: simpan bahasa ke config biar permanen
                let mut cfg = types::Config::load();
                cfg.language = if app.is_indo { "id".into() } else { "en".into() };
                let _ = cfg.save();
                app.state = AppState::Loading("Mengganti bahasa...".into());
                spawn_generate(app);
            }
            4 => {
                app.edit_input.clear();
                app.edit_cursor = 0;
                app.state = AppState::Manual;
            }
            5 => {
                app.state = AppState::Quit;
            }
            _ => {}
        },
        KeyCode::Char('e') => {
            app.edit_input = app.commit_msg.clone();
            app.edit_cursor = app.edit_input.chars().count();
            app.state = AppState::Editing;
        }
        KeyCode::Char('r') => {
            app.state = AppState::Loading("Membuat ulang...".into());
            spawn_generate(app);
        }
        KeyCode::Char('m') => {
            app.edit_input.clear();
            app.edit_cursor = 0;
            app.state = AppState::Manual;
        }
        KeyCode::Char('q') | KeyCode::Esc => {
            app.state = AppState::Quit;
        }
        _ => {}
    }
}

fn handle_input(key: &KeyCode, app: &mut App) -> io::Result<()> {
    // ponytail: cursor dalam char (bukan byte) biar unicode aman
    let len = app.edit_input.chars().count();
    let byte_at = |s: &str, ci: usize| s.char_indices().map(|(b, _)| b).nth(ci).unwrap_or(s.len());
    match key {
        KeyCode::Enter => {
            if !app.edit_input.trim().is_empty() {
                app.commit_msg = app.edit_input.clone();
                if matches!(app.state, AppState::Manual) {
                    if let Err(e) = git::commit(&app.commit_msg) {
                        eprintln!("Error: {}", e);
                    } else {
                        println!("✅ Commit berhasil!");
                    }
                    app.state = AppState::Quit;
                    return Ok(());
                }
            }
            app.state = AppState::Menu;
        }
        // ponytail: q == Esc di mode edit (batal ke menu)
        KeyCode::Char('q') => {
            app.state = AppState::Menu;
        }
        KeyCode::Char(c) => {
            let b = byte_at(&app.edit_input, app.edit_cursor);
            app.edit_input.insert(b, *c);
            app.edit_cursor += 1;
        }
        KeyCode::Backspace => {
            if app.edit_cursor > 0 {
                let b = byte_at(&app.edit_input, app.edit_cursor - 1);
                app.edit_input.remove(b);
                app.edit_cursor -= 1;
            }
        }
        KeyCode::Delete => {
            if app.edit_cursor < len {
                let b = byte_at(&app.edit_input, app.edit_cursor);
                app.edit_input.remove(b);
            }
        }
        KeyCode::Left => app.edit_cursor = app.edit_cursor.saturating_sub(1),
        KeyCode::Right => app.edit_cursor = (app.edit_cursor + 1).min(len),
        KeyCode::Up => {
            let (line, col) = pos_to_line_col(&app.edit_input, app.edit_cursor);
            if line > 0 {
                app.edit_cursor = line_col_to_pos(&app.edit_input, line - 1, col);
            }
        }
        KeyCode::Down => {
            let (line, col) = pos_to_line_col(&app.edit_input, app.edit_cursor);
            let total_lines = app.edit_input.lines().count();
            if line + 1 < total_lines {
                app.edit_cursor = line_col_to_pos(&app.edit_input, line + 1, col);
            }
        },
        KeyCode::Home => app.edit_cursor = 0,
        KeyCode::End => app.edit_cursor = len,
        KeyCode::Esc => {
            app.state = AppState::Menu;
        }
        _ => {}
    }
    Ok(())
}

fn print_leftover(msg: &str) {
    let msg = msg.trim_end();
    if msg.trim().is_empty() {
        return;
    }
    // ponytail: lebar ngikutin rumus box TUI (90% terminal, 48..64)
    let w = crossterm::terminal::size()
        .map(|(c, _)| (c as usize * 90 / 100).clamp(48, 64))
        .unwrap_or(56);
    println!("{}", "─".repeat(w));
    for line in msg.lines() {
        println!("  {}", line);
    }
    println!("{}", "─".repeat(w));
}

fn pos_to_line_col(s: &str, pos: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    for (i, c) in s.chars().enumerate() {
        if i >= pos {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn line_col_to_pos(s: &str, target_line: usize, target_col: usize) -> usize {
    let mut line = 0;
    let mut col = 0;
    for (i, c) in s.chars().enumerate() {
        if c == '\n' {
            line += 1;
            col = 0;
            continue;
        }
        if line == target_line && col >= target_col {
            return i;
        }
        col += 1;
    }
    s.len()
}

fn cleanup(terminal: &mut Terminal<CrosstermBackend<&mut io::Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), crossterm::terminal::LeaveAlternateScreen)?;
    execute!(terminal.backend_mut(), cursor::Show)?;
    Ok(())
}
