//! ratatui live dashboard for `byro-dbg --tui`.
//!
//! Four tabs:
//!   * **Metrics** — CPU / RAM / VRAM gauges + per-pass GPU times.
//!     Polled at 2 Hz to match the engine's `metrics_sample_system`
//!     refresh cadence.
//!   * **Entities** — list of every named entity (one-shot fetch on
//!     tab activation; `r` refetches).
//!   * **Loader** — form-shaped panel for queueing a NIF load
//!     against the engine's `PendingDebugLoadSlot`.
//!   * **Console** — free-form text input → `DebugRequest::Eval` →
//!     scrolling response log (parity with the REPL mode for
//!     anything the TUI doesn't expose explicitly).
//!
//! Networking lives on a worker thread that owns the `TcpStream`
//! and exchanges `(DebugRequest, DebugResponse)` pairs via channels.
//! The UI thread polls the response channel each frame and feeds the
//! results into the app state — no blocking I/O on the render loop.

use std::io;
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use byroredux_debug_protocol::{wire, DebugRequest, DebugResponse, EntityInfo};
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Tabs, Wrap};
use ratatui::Terminal;

/// Polling cadence for the `Metrics` request. The engine refreshes
/// the snapshot at 2 Hz, so anything faster on the client is wasted
/// — pick the same period.
const METRICS_POLL_PERIOD: Duration = Duration::from_millis(500);

/// Per-frame keyboard-poll window. Short enough that response-drain
/// and metric polls stay responsive without burning CPU at idle.
const INPUT_POLL_TIMEOUT: Duration = Duration::from_millis(50);

/// Tab cycle order — same enum is indexed by the `Tabs` widget and
/// drives the body render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Metrics,
    Entities,
    Loader,
    Console,
}

impl Tab {
    const ALL: [Tab; 4] = [Tab::Metrics, Tab::Entities, Tab::Loader, Tab::Console];

    fn title(self) -> &'static str {
        match self {
            Tab::Metrics => "Metrics",
            Tab::Entities => "Entities",
            Tab::Loader => "Loader",
            Tab::Console => "Console",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }

    fn cycle(self, forward: bool) -> Tab {
        let i = self.index();
        let n = Self::ALL.len();
        let next = if forward {
            (i + 1) % n
        } else {
            (i + n - 1) % n
        };
        Self::ALL[next]
    }
}

/// Loader-tab form state. Two fields today (path + label); cell-load
/// fields are deferred until a profile selector lands in Phase 5.
struct LoaderForm {
    path: String,
    label: String,
    /// Which field is currently focused (Tab cycles between them).
    focus: LoaderField,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoaderField {
    Path,
    Label,
}

impl LoaderForm {
    fn new() -> Self {
        Self {
            path: String::new(),
            label: String::new(),
            focus: LoaderField::Path,
        }
    }

    fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            LoaderField::Path => LoaderField::Label,
            LoaderField::Label => LoaderField::Path,
        };
    }

    fn focused_buf(&mut self) -> &mut String {
        match self.focus {
            LoaderField::Path => &mut self.path,
            LoaderField::Label => &mut self.label,
        }
    }

    /// Build the request — None when path is empty.
    fn build_request(&self) -> Option<DebugRequest> {
        if self.path.trim().is_empty() {
            return None;
        }
        Some(DebugRequest::LoadNif {
            path: self.path.trim().to_string(),
            label: if self.label.trim().is_empty() {
                None
            } else {
                Some(self.label.trim().to_string())
            },
        })
    }
}

struct ConsolePane {
    input: String,
    /// Scrolling log of (sent-line, response-summary) pairs. Bounded
    /// so a long session doesn't grow unbounded.
    history: Vec<String>,
}

const CONSOLE_HISTORY_CAP: usize = 200;

impl ConsolePane {
    fn new() -> Self {
        Self {
            input: String::new(),
            history: Vec::new(),
        }
    }

    fn push_line(&mut self, line: String) {
        self.history.push(line);
        if self.history.len() > CONSOLE_HISTORY_CAP {
            let overflow = self.history.len() - CONSOLE_HISTORY_CAP;
            self.history.drain(..overflow);
        }
    }
}

/// Whole-app state for the TUI run loop.
struct App {
    tab: Tab,
    metrics: Option<MetricsView>,
    entities: Option<Vec<EntityInfo>>,
    loader: LoaderForm,
    console: ConsolePane,
    status: String,
    should_quit: bool,
}

/// Flattened metrics snapshot for the UI — same fields as the
/// protocol response, copied at receive so the renderer doesn't have
/// to pattern-match the variant every frame.
struct MetricsView {
    sampled_at_secs: u64,
    cpu_pct: f32,
    ram_used_mb: u64,
    ram_total_mb: u64,
    process_ram_mb: u64,
    vram_used_mb: u64,
    vram_reserved_mb: u64,
    vram_budget_mb: u64,
    gpu_pass_ms: Vec<(String, f32)>,
}

impl App {
    fn new() -> Self {
        Self {
            tab: Tab::Metrics,
            metrics: None,
            entities: None,
            loader: LoaderForm::new(),
            console: ConsolePane::new(),
            status: "F1 help · Tab next · Shift+Tab prev · q quit".to_string(),
            should_quit: false,
        }
    }

    fn handle_response(&mut self, resp: DebugResponse) {
        match resp {
            DebugResponse::Metrics {
                sampled_at_secs,
                cpu_pct,
                ram_used_mb,
                ram_total_mb,
                process_ram_mb,
                vram_used_mb,
                vram_reserved_mb,
                vram_budget_mb,
                gpu_pass_ms,
            } => {
                self.metrics = Some(MetricsView {
                    sampled_at_secs,
                    cpu_pct,
                    ram_used_mb,
                    ram_total_mb,
                    process_ram_mb,
                    vram_used_mb,
                    vram_reserved_mb,
                    vram_budget_mb,
                    gpu_pass_ms,
                });
            }
            DebugResponse::EntityList { entities } => {
                self.entities = Some(entities);
            }
            DebugResponse::Ok => {
                self.status = "ok".to_string();
                self.console.push_line("→ Ok".to_string());
            }
            DebugResponse::Error { message } => {
                self.status = format!("error: {}", message);
                self.console.push_line(format!("→ Error: {}", message));
            }
            DebugResponse::Value { data } => {
                let s = serde_json::to_string(&data).unwrap_or_else(|_| "<bad json>".to_string());
                self.console.push_line(format!("→ {}", s));
            }
            other => {
                // Variant we don't render directly — surface a one-
                // line summary in the console so the operator still
                // sees something happened.
                self.console
                    .push_line(format!("→ {} (received)", variant_name(&other)));
            }
        }
    }

    /// Handle a key press; return a request to send if appropriate.
    fn handle_key(&mut self, key: KeyEvent) -> Option<DebugRequest> {
        // Global keys first — work on every tab.
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q')
                if !matches!(self.tab, Tab::Loader | Tab::Console) =>
            {
                self.should_quit = true;
                return None;
            }
            KeyCode::Esc => {
                self.should_quit = true;
                return None;
            }
            KeyCode::Tab => {
                // Loader tab consumes Tab for field focus; otherwise
                // cycle tabs.
                if self.tab == Tab::Loader {
                    self.loader.toggle_focus();
                    return None;
                }
                self.tab = self.tab.cycle(true);
                return self.on_tab_enter();
            }
            KeyCode::BackTab => {
                self.tab = self.tab.cycle(false);
                return self.on_tab_enter();
            }
            _ => {}
        }

        // Per-tab handling.
        match self.tab {
            Tab::Metrics => None,
            Tab::Entities => match key.code {
                KeyCode::Char('r') | KeyCode::Char('R') => Some(DebugRequest::ListEntities {
                    component: None,
                }),
                _ => None,
            },
            Tab::Loader => self.handle_loader_key(key),
            Tab::Console => self.handle_console_key(key),
        }
    }

    fn handle_loader_key(&mut self, key: KeyEvent) -> Option<DebugRequest> {
        match key.code {
            KeyCode::Char(c) => {
                self.loader.focused_buf().push(c);
                None
            }
            KeyCode::Backspace => {
                self.loader.focused_buf().pop();
                None
            }
            KeyCode::Enter => {
                let req = self.loader.build_request();
                if req.is_none() {
                    self.status = "Loader: path is empty".to_string();
                }
                req
            }
            _ => None,
        }
    }

    fn handle_console_key(&mut self, key: KeyEvent) -> Option<DebugRequest> {
        match key.code {
            KeyCode::Char(c) => {
                self.console.input.push(c);
                None
            }
            KeyCode::Backspace => {
                self.console.input.pop();
                None
            }
            KeyCode::Enter => {
                let text = self.console.input.trim().to_string();
                if text.is_empty() {
                    return None;
                }
                self.console.input.clear();
                self.console.push_line(format!("byro> {}", text));
                Some(DebugRequest::Eval { expr: text })
            }
            _ => None,
        }
    }

    /// Some tabs trigger a one-shot fetch on entry (Entities loads
    /// the list once). Returns the request to send, if any.
    fn on_tab_enter(&mut self) -> Option<DebugRequest> {
        match self.tab {
            Tab::Entities if self.entities.is_none() => Some(DebugRequest::ListEntities {
                component: None,
            }),
            _ => None,
        }
    }
}

fn variant_name(resp: &DebugResponse) -> &'static str {
    match resp {
        DebugResponse::Value { .. } => "Value",
        DebugResponse::EntityList { .. } => "EntityList",
        DebugResponse::ComponentList { .. } => "ComponentList",
        DebugResponse::SystemList { .. } => "SystemList",
        DebugResponse::Stats { .. } => "Stats",
        DebugResponse::Screenshot { .. } => "Screenshot",
        DebugResponse::ScreenshotSaved { .. } => "ScreenshotSaved",
        DebugResponse::Ok => "Ok",
        DebugResponse::Pong => "Pong",
        DebugResponse::Hierarchy { .. } => "Hierarchy",
        DebugResponse::SkinnedMesh { .. } => "SkinnedMesh",
        DebugResponse::Inspect { .. } => "Inspect",
        DebugResponse::Metrics { .. } => "Metrics",
        DebugResponse::GameProfiles { .. } => "GameProfiles",
        DebugResponse::AssetList { .. } => "AssetList",
        DebugResponse::Error { .. } => "Error",
    }
}

// ── Networking thread ──────────────────────────────────────────────

/// Spawn a worker thread that owns the TCP stream. Requests sent on
/// the returned `Sender` are written to the wire; responses arrive
/// on the `Receiver`. The thread exits when either channel closes
/// or a wire error occurs.
fn spawn_net_thread(
    mut stream: TcpStream,
) -> (Sender<DebugRequest>, Receiver<DebugResponse>) {
    let (req_tx, req_rx) = mpsc::channel::<DebugRequest>();
    let (resp_tx, resp_rx) = mpsc::channel::<DebugResponse>();
    thread::Builder::new()
        .name("byro-dbg-net".into())
        .spawn(move || {
            while let Ok(req) = req_rx.recv() {
                if wire::send(&mut stream, &req).is_err() {
                    break;
                }
                match wire::decode::<DebugResponse>(&mut stream) {
                    Ok(resp) => {
                        if resp_tx.send(resp).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        })
        .expect("spawn byro-dbg-net thread");
    (req_tx, resp_rx)
}

// ── Render ─────────────────────────────────────────────────────────

fn render(f: &mut ratatui::Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    let tabs = Tabs::new(
        Tab::ALL
            .iter()
            .map(|t| Line::from(t.title()))
            .collect::<Vec<_>>(),
    )
    .block(Block::default().borders(Borders::ALL).title("byro-dbg"))
    .select(app.tab.index())
    .highlight_style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );
    f.render_widget(tabs, chunks[0]);

    match app.tab {
        Tab::Metrics => render_metrics(f, chunks[1], app),
        Tab::Entities => render_entities(f, chunks[1], app),
        Tab::Loader => render_loader(f, chunks[1], app),
        Tab::Console => render_console(f, chunks[1], app),
    }

    let status = Paragraph::new(Line::from(Span::styled(
        app.status.as_str(),
        Style::default().fg(Color::DarkGray),
    )));
    f.render_widget(status, chunks[2]);
}

fn render_metrics(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let Some(m) = &app.metrics else {
        let p = Paragraph::new("Waiting for first metrics sample…").block(
            Block::default()
                .borders(Borders::ALL)
                .title("Metrics (2 Hz)"),
        );
        f.render_widget(p, area);
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // CPU gauge
            Constraint::Length(3), // RAM gauge
            Constraint::Length(3), // VRAM gauge
            Constraint::Min(0),    // GPU pass times list
        ])
        .split(area);

    // CPU% — clamp into [0, 100] for the gauge widget; the raw
    // value can exceed 100 on multi-core boxes (sysinfo reports
    // 0..N*100). Show the raw value in the label so multi-core
    // saturation is visible.
    let cpu_ratio = (m.cpu_pct.clamp(0.0, 100.0)) / 100.0;
    let cpu_gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("CPU"))
        .gauge_style(Style::default().fg(Color::Cyan))
        .label(format!("{:.1}%", m.cpu_pct))
        .ratio(cpu_ratio as f64);
    f.render_widget(cpu_gauge, chunks[0]);

    let ram_ratio = ratio(m.ram_used_mb, m.ram_total_mb);
    let ram_gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("RAM (system)"),
        )
        .gauge_style(Style::default().fg(Color::Green))
        .label(format!(
            "{} / {} MB  (process RSS {} MB)",
            m.ram_used_mb, m.ram_total_mb, m.process_ram_mb
        ))
        .ratio(ram_ratio);
    f.render_widget(ram_gauge, chunks[1]);

    let vram_ratio = ratio(m.vram_used_mb, m.vram_budget_mb);
    let vram_label = if m.vram_budget_mb > 0 {
        format!(
            "{} used / {} reserved / {} budget MB",
            m.vram_used_mb, m.vram_reserved_mb, m.vram_budget_mb
        )
    } else {
        format!(
            "{} used / {} reserved MB (budget unknown)",
            m.vram_used_mb, m.vram_reserved_mb
        )
    };
    let vram_gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("VRAM"))
        .gauge_style(Style::default().fg(Color::Magenta))
        .label(vram_label)
        .ratio(vram_ratio);
    f.render_widget(vram_gauge, chunks[2]);

    let mut lines = vec![Line::from(format!(
        "sampled at unix={}",
        m.sampled_at_secs
    ))];
    if m.gpu_pass_ms.is_empty() {
        lines.push(Line::from("(no GPU pass times reported)"));
    } else {
        lines.push(Line::from("GPU per-pass (ms):"));
        for (name, ms) in &m.gpu_pass_ms {
            lines.push(Line::from(format!("  {:<20} {:>6.3}", name, ms)));
        }
    }
    let para = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title("GPU passes"),
    );
    f.render_widget(para, chunks[3]);
}

/// Safe ratio for gauge widgets — returns 0.0 when `total` is zero
/// to avoid the NaN that would otherwise propagate into the
/// renderer. Clamps to `[0.0, 1.0]` so an over-budget value (used >
/// budget) doesn't break layout.
fn ratio(used: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    (used as f64 / total as f64).clamp(0.0, 1.0)
}

fn render_entities(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Entities  (r = refresh)");
    let items: Vec<ListItem> = match &app.entities {
        Some(list) if list.is_empty() => vec![ListItem::new("(no entities)")],
        Some(list) => list
            .iter()
            .map(|e| {
                let name = e.name.clone().unwrap_or_else(|| "<unnamed>".into());
                ListItem::new(format!("  {:>6}  {}", e.id, name))
            })
            .collect(),
        None => vec![ListItem::new("Press 'r' to fetch the entity list")],
    };
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn render_loader(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);

    let path_title = if app.loader.focus == LoaderField::Path {
        "Path  [Tab to focus label] *"
    } else {
        "Path  [Tab to focus label]"
    };
    let path = Paragraph::new(app.loader.path.as_str())
        .block(Block::default().borders(Borders::ALL).title(path_title));
    f.render_widget(path, chunks[0]);

    let label_title = if app.loader.focus == LoaderField::Label {
        "Label (optional)  *"
    } else {
        "Label (optional)"
    };
    let label = Paragraph::new(app.loader.label.as_str())
        .block(Block::default().borders(Borders::ALL).title(label_title));
    f.render_widget(label, chunks[1]);

    let help = Paragraph::new(vec![
        Line::from("Enter to queue load.  Path may be loose absolute or BSA-relative."),
        Line::from("The active engine must have the relevant --bsa archive loaded."),
        Line::from(""),
        Line::from(Span::styled(
            "Cell-load form lands in Phase 5 alongside the game profile registry.",
            Style::default().fg(Color::DarkGray),
        )),
    ])
    .wrap(Wrap { trim: true })
    .block(Block::default().borders(Borders::ALL).title("Load NIF"));
    f.render_widget(help, chunks[2]);
}

fn render_console(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(area);

    // Bottom: input line. Render last so the input is visible at the
    // bottom of the screen, the way a shell looks.
    let lines: Vec<Line> = app
        .console
        .history
        .iter()
        .map(|l| Line::from(l.as_str()))
        .collect();
    let log = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Output"))
        .wrap(Wrap { trim: false });
    f.render_widget(log, chunks[0]);

    let input = Paragraph::new(Line::from(vec![
        Span::styled("byro> ", Style::default().fg(Color::Yellow)),
        Span::raw(&app.console.input),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Input (Enter to send)"),
    );
    f.render_widget(input, chunks[1]);
}

// ── Entry + lifecycle ──────────────────────────────────────────────

/// Run the TUI against an already-connected stream. Returns once
/// the operator quits (q / Esc) or the engine drops the connection.
pub fn run(stream: TcpStream) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (req_tx, resp_rx) = spawn_net_thread(stream);
    let mut app = App::new();
    let mut last_metrics_poll = Instant::now() - METRICS_POLL_PERIOD;

    let result = (|| -> io::Result<()> {
        loop {
            // Drain any pending responses.
            loop {
                match resp_rx.try_recv() {
                    Ok(resp) => app.handle_response(resp),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        app.status = "engine disconnected".to_string();
                        app.should_quit = true;
                        break;
                    }
                }
            }

            // Periodic metrics poll.
            if app.tab == Tab::Metrics && last_metrics_poll.elapsed() >= METRICS_POLL_PERIOD {
                let _ = req_tx.send(DebugRequest::Metrics);
                last_metrics_poll = Instant::now();
            }

            terminal.draw(|f| render(f, &app))?;

            if event::poll(INPUT_POLL_TIMEOUT)? {
                if let Event::Key(key) = event::read()? {
                    if let Some(req) = app.handle_key(key) {
                        let _ = req_tx.send(req);
                    }
                }
            }

            if app.should_quit {
                break;
            }
        }
        Ok(())
    })();

    // Always restore the terminal even on error.
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    result
}
