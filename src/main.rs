use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use std::{
    io,
    path::Path,
    process::{Command, Stdio},
};
use walkdir::WalkDir;

#[derive(Parser)]
struct Args {
    #[arg(short, long)]
    print: bool,
}
enum Mode {
    Select,
    Show(String),
}
struct App {
    entries: Vec<String>,
    filtered: Vec<String>,
    selected: usize,
    search: String,
    mode: Mode,
}

impl App {
    fn new(entries: Vec<String>) -> Self {
        Self {
            filtered: entries.clone(),
            entries,
            selected: 0,
            search: String::new(),
            mode: Mode::Select,
        }
    }

    fn filter(&mut self) {
        if self.search.is_empty() {
            self.filtered = self.entries.clone();
        } else {
            let query = self.search.to_lowercase();
            self.filtered = self
                .entries
                .iter()
                .filter(|e| e.to_lowercase().contains(&query))
                .cloned()
                .collect();
        }

        if self.selected >= self.filtered.len() {
            self.selected = 0;
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let entries = load_pass_entries()?;

    enable_raw_mode()?;

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, App::new(entries), args.print);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut app: App,
    print_only: bool,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, &app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match key.code {
                KeyCode::Char(c) => {
                    app.search.push(c);
                    app.filter();
                }

                KeyCode::Backspace => {
                    app.search.pop();
                    app.filter();
                }

                KeyCode::Down => {
                    if !app.filtered.is_empty() {
                        app.selected = (app.selected + 1) % app.filtered.len();
                    }
                }

                KeyCode::Up => {
                    if !app.filtered.is_empty() {
                        if app.selected == 0 {
                            app.selected = app.filtered.len() - 1;
                        } else {
                            app.selected -= 1;
                        }
                    }
                }

                KeyCode::Enter => match &app.mode {
                    Mode::Select => {
                        if let Some(entry) = app.filtered.get(app.selected) {
                            let secret = get_secret(entry)?;

                            if print_only {
                                app.mode = Mode::Show(secret);
                            } else {
                                pass_to_clipboard(secret)?;
                                break;
                            }
                        }
                    }

                    Mode::Show(_) => {
                        break;
                    }
                },

                KeyCode::Esc => break,

                _ => {}
            }
        }
    }

    Ok(())
}

fn ui(f: &mut Frame, app: &App) {
    match &app.mode {
        Mode::Show(password) => {
            let block = Paragraph::new(password.as_str()).block(
                Block::default()
                    .title("Password (press Enter to exit)")
                    .borders(Borders::ALL),
            );

            f.render_widget(block, f.size());
            return;
        }

        Mode::Select => {}
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(f.size());

    let search = Paragraph::new(app.search.as_str())
        .block(Block::default().title("Search").borders(Borders::ALL));

    f.render_widget(search, layout[0]);

    let items: Vec<ListItem> = app
        .filtered
        .iter()
        .map(|e| ListItem::new(e.clone()))
        .collect();

    let list = List::new(items)
        .block(Block::default().title("Passwords").borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");

    let mut state = ListState::default();

    if !app.filtered.is_empty() {
        state.select(Some(app.selected));
    }

    f.render_stateful_widget(list, layout[1], &mut state);
}

fn load_pass_entries() -> Result<Vec<String>> {
    let home = std::env::var("HOME")?;
    let store = Path::new(&home).join(".password-store");

    let mut entries = Vec::new();

    for entry in WalkDir::new(store) {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("gpg") {
            let relative = strip_store_prefix(path)?;
            let clean = relative.trim_end_matches(".gpg");
            entries.push(clean.to_string());
        }
    }

    entries.sort();

    Ok(entries)
}

fn strip_store_prefix(path: &Path) -> Result<String> {
    let home = std::env::var("HOME")?;
    let store = Path::new(&home).join(".password-store");

    let relative = path
        .strip_prefix(store)
        .context("failed to strip password-store prefix")?;

    Ok(relative.to_string_lossy().to_string())
}

fn get_secret(entry: &str) -> Result<String> {
    let output = Command::new("pass")
        .arg("show")
        .arg(entry)
        .output()
        .context("failed to run pass")?;

    if !output.status.success() {
        anyhow::bail!("pass command failed");
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn pass_to_clipboard(secret: String) -> Result<()> {
    let filtered = secret
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut wl_copy = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .spawn()
        .context("failed to launch wl-copy")?;

    use std::io::Write;

    wl_copy
        .stdin
        .as_mut()
        .unwrap()
        .write_all(filtered.as_bytes())?;

    wl_copy.wait()?;

    Ok(())
}
