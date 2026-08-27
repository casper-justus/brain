use std::io::{self, Stdout};
use std::time::Duration;

use brain_core::index::{self, IndexEntry};
use brain_core::note::{self, NoteKind};
use brain_core::store::Store;
use brain_core::retrieve;
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::{Frame, Terminal};

pub fn run(store: Store) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    let mut app = App::new(store).map_err(|e| io::Error::other(e.to_string()))?;
    app.run_loop(&mut terminal)?;
    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    Ok(())
}

struct App {
    store: Store,
    entries: Vec<IndexEntry>,
    list_state: ListState,
    preview: String,
    preview_path: Option<std::path::PathBuf>,
    input: String,
    mode: Mode,
    status: String,
    ask_result: Option<String>,
}

enum Mode {
    Browse,
    Search,
    Ask,
    Capture,
}

impl App {
    fn new(store: Store) -> anyhow::Result<Self> {
        let mut app = App {
            store,
            entries: Vec::new(),
            list_state: ListState::default(),
            preview: String::new(),
            preview_path: None,
            input: String::new(),
            mode: Mode::Browse,
            status: String::new(),
            ask_result: None,
        };
        app.refresh()?;
        if !app.entries.is_empty() {
            app.list_state.select(Some(0));
            app.load_preview(0);
        }
        Ok(app)
    }

    fn refresh(&mut self) -> anyhow::Result<()> {
        let idx = index::build_index(&self.store)?;
        let mut entries = idx.docs.clone();
        entries.sort_by(|a, b| b.created.cmp(&a.created));
        self.entries = entries;
        index::save_index(&idx, &self.store.index_dir()?)?;
        Ok(())
    }

    fn load_preview(&mut self, i: usize) {
        if let Some(entry) = self.entries.get(i) {
            match note::read_note(&entry.path) {
                Ok(n) => {
                    self.preview = format!(
                        "{}  ·  {}\n{}\n\n{}",
                        entry.title,
                        entry.created,
                        "-".repeat(50),
                        n.body
                    );
                    self.preview_path = Some(entry.path.clone());
                }
                Err(e) => {
                    self.preview = format!("error reading: {e}");
                }
            }
        }
    }

    fn run_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
        loop {
            terminal.draw(|f| self.render(f))?;
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if self.handle_key(key)? {
                        return Ok(());
                    }
                }
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> io::Result<bool> {
        match self.mode {
            Mode::Search | Mode::Ask => match key.code {
                KeyCode::Esc => {
                    self.mode = Mode::Browse;
                    self.input.clear();
                    self.status.clear();
                    self.ask_result = None;
                }
                KeyCode::Enter => {
                    let query = self.input.trim().to_string();
                    if query.is_empty() {
                        self.mode = Mode::Browse;
                        return Ok(false);
                    }
                    match self.mode {
                        Mode::Search => {
                            match index::search(
                                &index::load_index(&self.store.index_dir().unwrap_or_default())
                                    .ok()
                                    .flatten()
                                    .unwrap_or_default(),
                                &query,
                            ) {
                                Ok(hits) => {
                                    let mut list: Vec<IndexEntry> =
                                        hits.into_iter().map(|h| h.entry).collect();
                                    list.dedup_by(|a, b| a.path == b.path);
                                    self.entries = list;
                                    self.status = format!("{} result(s) for \"{query}\"", self.entries.len());
                                    if !self.entries.is_empty() {
                                        self.list_state.select(Some(0));
                                        self.load_preview(0);
                                    }
                                }
                                Err(e) => self.status = format!("search error: {e}"),
                            }
                            self.mode = Mode::Browse;
                        }
                        Mode::Ask => match retrieve::ask(
                            &self.store,
                            &index::load_index(&self.store.index_dir().unwrap_or_default())
                                .ok()
                                .flatten()
                                .unwrap_or_default(),
                            &query,
                        ) {
                            Ok(ans) => {
                                self.ask_result = Some(format!(
                                    "{}\n\n{}",
                                    ans.synthesis,
                                    ans.chunks
                                        .iter()
                                        .map(|c| format!("[{}]\n{}", c.citation, c.excerpt))
                                        .collect::<Vec<_>>()
                                        .join("\n\n")
                                ));
                                self.preview = self.ask_result.clone().unwrap_or_default();
                                self.preview_path = None;
                                self.status = format!("ask: \"{query}\"");
                            }
                            Err(e) => self.status = format!("ask error: {e}"),
                        },
                        _ => {}
                    }
                }
                KeyCode::Char(c) => {
                    self.input.push(c);
                }
                KeyCode::Backspace => {
                    self.input.pop();
                }
                _ => {}
            },
            Mode::Browse => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
                KeyCode::Char('j') | KeyCode::Down => self.nav(1),
                KeyCode::Char('k') | KeyCode::Up => self.nav(-1),
                KeyCode::Char('g') => {
                    self.list_state.select(Some(0));
                    self.load_preview(0);
                }
                KeyCode::Char('G') => {
                    if !self.entries.is_empty() {
                        let last = self.entries.len() - 1;
                        self.list_state.select(Some(last));
                        self.load_preview(last);
                    }
                }
                KeyCode::Char('/') => {
                    self.mode = Mode::Search;
                    self.input.clear();
                    self.status = "search: ".into();
                }
                KeyCode::Char('a') => {
                    self.mode = Mode::Ask;
                    self.input.clear();
                    self.status = "ask: ".into();
                }
                KeyCode::Char('e') => {
                    if let Some(path) = self.preview_path.clone() {
                        let editor = self.store.cfg.editor();
                        drop_terminal(editor.to_string(), &path)?
                    }
                }
                KeyCode::Char('r') => {
                    match self.refresh() {
                        Ok(()) => self.status = "re-indexed".into(),
                        Err(e) => self.status = format!("reindex error: {e}"),
                    }
                }
                KeyCode::Char('n') => {
                    // new capture: switch to search mode; on Enter writes a capture
                    self.mode = Mode::Capture;
                    self.input.clear();
                    self.status = "new capture: ".into();
                }
                _ => {}
            },
            Mode::Capture => match key.code {
                KeyCode::Esc => {
                    self.mode = Mode::Browse;
                    self.input.clear();
                    self.status.clear();
                }
                KeyCode::Enter => {
                    let text = self.input.trim().to_string();
                    self.mode = Mode::Browse;
                    self.input.clear();
                    self.status.clear();
                    if !text.is_empty() {
                        let note = brain_cli_snap(&self.store, text);
                        self.status = match note {
                            Ok(p) => format!("captured: {}", p.display()),
                            Err(e) => format!("capture error: {e}"),
                        };
                        let _ = self.refresh();
                        self.list_state.select(Some(0));
                        self.load_preview(0);
                    }
                }
                KeyCode::Char(c) => self.input.push(c),
                KeyCode::Backspace => {
                    self.input.pop();
                }
                _ => {}
            },
        }
        Ok(false)
    }

    fn nav(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        let len = self.entries.len() as isize;
        let cur = self
            .list_state
            .selected()
            .map(|i| i as isize)
            .unwrap_or(0);
        let next = (cur + delta).clamp(0, len - 1) as usize;
        self.list_state.select(Some(next));
        self.load_preview(next);
    }

    fn render(&mut self, f: &mut Frame<'_>) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(f.area());

        let header = Line::from(Span::styled(
            " brain — your second brain",
            Style::default().add_modifier(Modifier::BOLD),
        ));
        f.render_widget(header, chunks[0]);

        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(chunks[1]);

        self.render_list(f, body[0]);
        self.render_preview(f, body[1]);

        let hint = Line::from(Span::styled(
            " j/k move · / search · a ask · n new · e edit · r reindex · q quit",
            Style::default().fg(Color::DarkGray),
        ));
        f.render_widget(hint, chunks[2]);

        let prompt = match self.mode {
            Mode::Search => format!("search> {}", self.input),
            Mode::Ask => format!("ask> {}", self.input),
            Mode::Capture => format!("new> {}", self.input),
            Mode::Browse => self.status.clone(),
        };
        f.render_widget(
            Paragraph::new(prompt).style(Style::default().fg(Color::Cyan)),
            chunks[3],
        );
    }

    fn render_list(&mut self, f: &mut Frame<'_>, area: Rect) {
        let items: Vec<ListItem> = self
            .entries
            .iter()
            .map(|e| {
                let kind = e.kind.clone();
                let color = match kind.as_str() {
                    "dev" => Color::Blue,
                    "link" => Color::Magenta,
                    "journal" => Color::Green,
                    "session" => Color::Yellow,
                    _ => Color::Gray,
                };
                let title = e.title.clone();
                ListItem::new(Line::from(vec![
                    Span::styled(kind, Style::default().fg(color)),
                    Span::raw("  "),
                    Span::raw(title),
                ]))
            })
            .collect();
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" notes ({}) ", self.entries.len())),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );
        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_preview(&mut self, f: &mut Frame<'_>, area: Rect) {
        let title = self
            .preview_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "(no selection)".into());
        let paragraph = Paragraph::new(self.preview.clone())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(Span::styled(
                        format!(" {title} "),
                        Style::default().fg(Color::White),
                    )),
            )
            .wrap(ratatui::widgets::Wrap { trim: false });
        f.render_widget(paragraph, area);
    }
}

fn brain_cli_snap(store: &Store, text: String) -> anyhow::Result<std::path::PathBuf> {
    let now = chrono::Local::now();
    let dir = store.kind_dir(NoteKind::Capture);
    let fname = note::filename_for(NoteKind::Capture, &now, None);
    let path = dir.join(fname);
    let meta = note::NoteMeta {
        kind: NoteKind::Capture,
        created: now,
        tags: Vec::new(),
        title: None,
        extra: Default::default(),
    };
    let n = note::Note {
        path: path.clone(),
        meta,
        body: text,
    };
    note::write_note(&n)?;
    Ok(path)
}

fn drop_terminal(editor: String, path: &std::path::Path) -> io::Result<()> {
    // temporarily leave the alternate screen so the editor can attach to the TTY
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    let status = std::process::Command::new(&editor).arg(path).status();
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => Err(io::Error::other("editor exited with failure")),
        Err(e) => Err(io::Error::other(format!("failed to launch editor: {e}"))),
    }
}
