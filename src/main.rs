mod ai;
mod app;
mod audit;
mod doctor;
mod git;
mod heuristic;
mod hooks;
mod linter;
mod tui;
mod types;
mod update;

use app::{App, AppState};
use clap::{Parser, Subcommand};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{cursor, execute};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, Write};
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(
    name = "clg",
    about = "Generate and lint commit messages",
    disable_version_flag = true
)]
struct Cli {
    /// Print version
    #[arg(short = 'v', long = "version")]
    version: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Generate a commit message from staged changes
    Generate {
        /// Commit message file path (automatically filled by git hook)
        commit_msg_file: Option<String>,
        /// Auto-commit without interactive prompt
        #[arg(short = 'y', long = "yes")]
        yes: bool,
        /// Use heuristic generation instead of AI
        #[arg(short = 'H', long = "heuristic")]
        heuristic: bool,
    },
    /// Validate a commit message against conventional commit rules
    Lint {
        /// Commit message to validate
        message: String,
    },
    /// Install git hook in the current repository
    Init,
    /// Remove git hook from the current repository
    Uninstall,
    /// Analyze recent commit history for conventional commit compliance
    Audit {
        /// Number of commits to analyze
        #[arg(short = 'n', long = "number", default_value_t = 20)]
        number: usize,
    },
    /// Interactive setup for AI provider configuration
    Config,
    /// Check environment, config, and API connection
    Doctor,
    /// Check for a newer release and update via npm
    Update {
        /// Only check, do not install
        #[arg(long)]
        check: bool,
    },
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();
    if cli.version {
        println!("{}", update::version_line());
        return Ok(());
    }
    if !matches!(&cli.command, Some(Commands::Update { .. })) {
        update::auto_update();
    }

    match cli.command.unwrap_or(Commands::Generate {
        commit_msg_file: None,
        yes: false,
        heuristic: false,
    }) {
        Commands::Generate {
            commit_msg_file,
            yes,
            heuristic,
        } => cmd_generate(commit_msg_file, yes, heuristic),
        Commands::Lint { message } => {
            let config = types::Config::load();
            let errors = linter::validate(&message, &config);
            if errors.is_empty() {
                println!("✓ Valid commit message");
                Ok(())
            } else {
                eprintln!("✗ Invalid commit message:\n");
                for error in errors {
                    eprintln!("  [{}] {}", error.rule, error.message);
                }
                std::process::exit(1);
            }
        }
        Commands::Init => {
            let cwd = std::env::current_dir()?;
            match hooks::install(&cwd) {
                Ok(path) => {
                    println!("✓ Git hook installed successfully!");
                    println!("  Location: {}", path);
                    println!("\nThe hook will validate commit messages automatically.");
                    println!("To generate commit messages, run: clg");
                    Ok(())
                }
                Err(error) => {
                    eprintln!("Error installing git hook: {}", error);
                    std::process::exit(1);
                }
            }
        }
        Commands::Uninstall => {
            let cwd = std::env::current_dir()?;
            match hooks::uninstall(&cwd) {
                Ok(true) => {
                    println!("✓ Git hook uninstalled successfully!");
                    Ok(())
                }
                Ok(false) => {
                    println!("No git hook found to uninstall.");
                    Ok(())
                }
                Err(error) => {
                    eprintln!("Error uninstalling git hook: {}", error);
                    std::process::exit(1);
                }
            }
        }
        Commands::Audit { number } => {
            if number < 1 {
                eprintln!("Error: --number must be a positive integer");
                std::process::exit(1);
            }
            let config = types::Config::load();
            match audit::analyze(number, &config) {
                Ok(analyses) => {
                    print!("{}", audit::format_report(&analyses));
                    let invalid = analyses.iter().any(|a| !a.is_valid());
                    if invalid {
                        std::process::exit(1);
                    }
                    Ok(())
                }
                Err(error) => {
                    eprintln!("Error analyzing commit history: {}", error);
                    std::process::exit(1);
                }
            }
        }
        Commands::Config => setup_config(),
        Commands::Doctor => {
            let config = types::Config::load();
            doctor::run(&config);
            Ok(())
        }
        Commands::Update { check } => match update::run(check) {
            Ok(()) => Ok(()),
            Err(error) => {
                eprintln!("Error: {}", error);
                std::process::exit(1);
            }
        },
    }
}

fn cmd_generate(commit_msg_file: Option<String>, yes: bool, heuristic: bool) -> io::Result<()> {
    let config = types::Config::load();
    let offline = heuristic || config.apiKey.trim().is_empty();

    if yes {
        let message = if offline {
            heuristic::generate()
        } else {
            let diff = match git::staged_diff(&config) {
                Ok(diff) => diff,
                Err(error) => {
                    eprintln!("Error: {}", error);
                    return Ok(());
                }
            };
            generate_ai(&config, &diff, None)
        };
        return deliver(&message, commit_msg_file);
    }

    run_tui(config, commit_msg_file, heuristic)
}

fn deliver(message: &str, commit_msg_file: Option<String>) -> io::Result<()> {
    if let Some(path) = commit_msg_file {
        std::fs::write(&path, format!("{}\n", message))?;
        print_leftover(message);
        return Ok(());
    }
    git::commit(message).map_err(io::Error::other)?;
    println!("Committed: {}", message);
    println!("\nDon't forget to push your commits!");
    Ok(())
}

fn generate_ai(config: &types::Config, diff: &str, previous: Option<&str>) -> String {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("Runtime error: {}. Memakai heuristic.", error);
            return heuristic::generate();
        }
    };
    runtime
        .block_on(ai::generate_commit(
            diff,
            &config.apiKey,
            &config.baseURL,
            &config.model,
            config.is_indo(),
            previous,
        ))
        .unwrap_or_else(|error| {
            eprintln!("AI generation failed: {}. Memakai heuristic.", error);
            heuristic::generate()
        })
}

fn run_tui(
    config: types::Config,
    commit_msg_file: Option<String>,
    heuristic: bool,
) -> io::Result<()> {
    let diff = match git::staged_diff(&config) {
        Ok(diff) => diff,
        Err(error) => {
            eprintln!("Error: {}", error);
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
        model: config.model.clone(),
        base_url: config.baseURL.clone(),
        api_key: config.apiKey.clone(),
        is_indo: config.is_indo(),
        selected_option: 0,
        state: AppState::Loading("Membuat pesan commit...".into()),
        diff,
        pending: None,
        rt,
        ticker: std::time::Instant::now(),
        heuristic,
        commit_msg_file,
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
            if key.code == KeyCode::Char('c')
                && key.modifiers.contains(event::KeyModifiers::CONTROL)
            {
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
        format!(
            "{}...{}",
            &cfg.apiKey[..8],
            &cfg.apiKey[cfg.apiKey.len() - 8..]
        )
    } else if cfg.apiKey.is_empty() {
        String::new()
    } else {
        "********".to_string()
    };
    cfg.apiKey =
        ask("Kunci API Groq", &masked).map(|s| if s == masked { cfg.apiKey.clone() } else { s })?;
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
    let offline = app.heuristic || key.trim().is_empty();
    let previous = if app.commit_msg.trim().is_empty() {
        None
    } else {
        Some(app.commit_msg.clone())
    };
    app.pending = Some(rx);

    app.rt.spawn(async move {
        let result = if offline {
            Ok(heuristic::generate())
        } else {
            match ai::generate_commit(&diff, &key, &base, &model, is_indo, previous.as_deref())
                .await
            {
                Ok(message) => Ok(message),
                Err(_) => Ok(heuristic::generate()),
            }
        };
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
                finish_commit(app);
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
                cfg.language = if app.is_indo {
                    "id".into()
                } else {
                    "en".into()
                };
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

fn finish_commit(app: &mut App) {
    if let Some(path) = app.commit_msg_file.clone() {
        match std::fs::write(&path, format!("{}\n", app.commit_msg)) {
            Ok(()) => println!("Commit message ditulis ke {}", path),
            Err(error) => eprintln!("Error: {}", error),
        }
    } else if let Err(error) = git::commit(&app.commit_msg) {
        eprintln!("Error: {}", error);
    } else {
        println!("✅ Commit berhasil!");
    }
    app.state = AppState::Quit;
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
                    finish_commit(app);
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
        }
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
    // wrap display-width + hanging indent bullet, biar rapi di dalam box
    for line in tui::wrap_display(msg, w.saturating_sub(2)) {
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
    execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;
    execute!(terminal.backend_mut(), cursor::Show)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(args)
    }

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn no_args_defaults_to_generate() {
        let cli = parse(&["clg"]).unwrap();
        assert!(cli.command.is_none());
        assert!(!cli.version);
    }

    #[test]
    fn parses_generate_flags() {
        let cli = parse(&["clg", "generate", "-y", "-H", "msg.txt"]).unwrap();
        match cli.command {
            Some(Commands::Generate {
                commit_msg_file,
                yes,
                heuristic,
            }) => {
                assert_eq!(commit_msg_file.as_deref(), Some("msg.txt"));
                assert!(yes);
                assert!(heuristic);
            }
            _ => panic!("expected generate command"),
        }
    }

    #[test]
    fn parses_lint_audit_and_version() {
        assert!(matches!(
            parse(&["clg", "lint", "feat: add x"]).unwrap().command,
            Some(Commands::Lint { .. })
        ));
        match parse(&["clg", "audit", "-n", "5"]).unwrap().command {
            Some(Commands::Audit { number }) => assert_eq!(number, 5),
            _ => panic!("expected audit"),
        }
        assert!(parse(&["clg", "-v"]).unwrap().version);
        assert!(parse(&["clg", "--version"]).unwrap().version);
    }

    #[test]
    fn parses_init_uninstall_config_doctor() {
        for (arg, expected) in [
            ("init", "init"),
            ("uninstall", "uninstall"),
            ("config", "config"),
            ("doctor", "doctor"),
            ("update", "update"),
        ] {
            let cli = parse(&["clg", arg]).unwrap();
            let matched = matches!(
                (&cli.command, expected),
                (Some(Commands::Init), "init")
                    | (Some(Commands::Uninstall), "uninstall")
                    | (Some(Commands::Config), "config")
                    | (Some(Commands::Doctor), "doctor")
                    | (Some(Commands::Update { .. }), "update")
            );
            assert!(matched, "failed to parse {}", arg);
        }
    }

    #[test]
    fn help_is_available() {
        let error = parse(&["clg", "--help"]).unwrap_err();
        assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
    }
}
