//! Interactive TUI for WinKeyer settings and status.
//!
//! Usage: cargo run --example tui -- /dev/ttyUSB0

use std::io::stdout;
use std::time::Duration;

use crossterm::event::{
    Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::ExecutableCommand;
use futures::StreamExt;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};
use tokio::sync::mpsc;

use winkey::{Keyer, KeyerEvent, KeyerStatus, PaddleMode, PinConfig, WinKeyerBuilder};

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Settings,
    Input,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PaddleModeValue {
    IambicA,
    IambicB,
    Ultimatic,
    Bug,
}

impl PaddleModeValue {
    fn label(self) -> &'static str {
        match self {
            Self::IambicA => "Iambic A",
            Self::IambicB => "Iambic B",
            Self::Ultimatic => "Ultimatic",
            Self::Bug => "Bug",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::IambicA => Self::IambicB,
            Self::IambicB => Self::Ultimatic,
            Self::Ultimatic => Self::Bug,
            Self::Bug => Self::IambicA,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::IambicA => Self::Bug,
            Self::IambicB => Self::IambicA,
            Self::Ultimatic => Self::IambicB,
            Self::Bug => Self::Ultimatic,
        }
    }

    fn to_protocol(self) -> PaddleMode {
        match self {
            Self::IambicA => PaddleMode::IambicA,
            Self::IambicB => PaddleMode::IambicB,
            Self::Ultimatic => PaddleMode::Ultimatic,
            Self::Bug => PaddleMode::Bug,
        }
    }
}

struct App {
    // Settings values
    speed: u8,
    weight: u8,
    sidetone: u16,
    sidetone_vol: u8,
    sidetone_on: bool,
    farnsworth: u8,
    ratio: u8,
    paddle_mode: PaddleModeValue,
    ptt_on: bool,
    ptt_lead_in: u8,
    ptt_tail: u8,
    hang_time: u8,
    tune: bool,
    pause: bool,

    // UI state
    focus: Focus,
    selected: usize,
    input_buf: String,
    echo_buf: String,

    // Status
    status: KeyerStatus,
    speed_pot: Option<u8>,

    // Keyer info
    keyer_name: String,
    keyer_port: String,

    quit: bool,
}

const NUM_SETTINGS: usize = 14;

impl App {
    fn new(keyer_name: String, keyer_port: String, speed: u8) -> Self {
        Self {
            speed,
            weight: 50,
            sidetone: 800,
            sidetone_vol: 4,
            sidetone_on: true,
            farnsworth: 0,
            ratio: 50,
            paddle_mode: PaddleModeValue::IambicB,
            ptt_on: true,
            ptt_lead_in: 0,
            ptt_tail: 0,
            hang_time: 0,
            tune: false,
            pause: false,

            focus: Focus::Settings,
            selected: 0,
            input_buf: String::new(),
            echo_buf: String::new(),

            status: KeyerStatus {
                xoff: false,
                breakin: false,
                busy: false,
                keydown: false,
                waiting: false,
            },
            speed_pot: None,

            keyer_name,
            keyer_port,

            quit: false,
        }
    }

    fn setting_label(&self, idx: usize) -> &'static str {
        match idx {
            0 => "Speed",
            1 => "Weight",
            2 => "Sidetone Freq",
            3 => "ST Volume",
            4 => "Sidetone",
            5 => "Farnsworth",
            6 => "Dit/Dah Ratio",
            7 => "Paddle Mode",
            8 => "PTT",
            9 => "PTT Lead-in",
            10 => "PTT Tail",
            11 => "PTT Hang Time",
            12 => "Tune",
            13 => "Pause",
            _ => "",
        }
    }

    fn setting_value(&self, idx: usize) -> String {
        match idx {
            0 => format!("{} WPM", self.speed),
            1 => format!("{}", self.weight),
            2 => format!("{} Hz", self.sidetone),
            3 => match self.sidetone_vol {
                1..=2 => format!("{} (low)", self.sidetone_vol),
                _ => format!("{} (normal)", self.sidetone_vol),
            },
            4 => if self.sidetone_on { "ON" } else { "OFF" }.into(),
            5 => {
                if self.farnsworth == 0 {
                    "0 (off)".into()
                } else {
                    format!("{}", self.farnsworth)
                }
            }
            6 => format!("{}", self.ratio),
            7 => self.paddle_mode.label().into(),
            8 => if self.ptt_on { "ON" } else { "OFF" }.into(),
            9 => format!("{} ({}ms)", self.ptt_lead_in, self.ptt_lead_in as u16 * 10),
            10 => format!("{} ({}ms)", self.ptt_tail, self.ptt_tail as u16 * 10),
            11 => match self.hang_time {
                0 => "1.0 wordspace".into(),
                1 => "1.33 wordspace".into(),
                2 => "1.67 wordspace".into(),
                _ => "2.0 wordspace".into(),
            },
            12 => if self.tune { "ON" } else { "OFF" }.into(),
            13 => if self.pause { "ON" } else { "OFF" }.into(),
            _ => String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Events funneled into a single channel
// ---------------------------------------------------------------------------

enum AppEvent {
    Terminal(Event),
    Keyer(KeyerEvent),
    Tick,
}

// ---------------------------------------------------------------------------
// UI rendering
// ---------------------------------------------------------------------------

fn ui(frame: &mut Frame, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(12),    // top: settings + status
            Constraint::Length(3),  // echo
            Constraint::Length(3),  // input
            Constraint::Length(1),  // help bar
        ])
        .split(frame.area());

    // -- Top: settings (left) + status (right) --
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(outer[0]);

    // Settings table
    let header_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let rows: Vec<Row> = (0..NUM_SETTINGS)
        .map(|i| {
            let marker = if app.focus == Focus::Settings && app.selected == i {
                ">"
            } else {
                " "
            };
            let style = if app.focus == Focus::Settings && app.selected == i {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Row::new(vec![
                marker.to_string(),
                app.setting_label(i).to_string(),
                app.setting_value(i),
            ])
            .style(style)
        })
        .collect();

    let widths = [Constraint::Length(1), Constraint::Length(15), Constraint::Min(14)];
    let table = Table::new(rows, widths)
        .header(Row::new(vec!["", "Setting", "Value"]).style(header_style))
        .block(Block::default().borders(Borders::ALL).title(format!(
            " {} {} ",
            app.keyer_name, app.keyer_port
        )));
    frame.render_widget(table, top[0]);

    // Status panel
    let yn = |b: bool| if b { "yes" } else { "no " };
    let pot_str = match app.speed_pot {
        Some(w) => format!("{w} WPM"),
        None => "-- WPM".into(),
    };
    let status_text = vec![
        Line::from(vec![
            Span::styled("Busy: ", Style::default().fg(Color::Gray)),
            Span::styled(
                yn(app.status.busy),
                if app.status.busy {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::Green)
                },
            ),
            Span::raw("   "),
            Span::styled("Key: ", Style::default().fg(Color::Gray)),
            Span::styled(
                yn(app.status.keydown),
                if app.status.keydown {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::Green)
                },
            ),
        ]),
        Line::from(vec![
            Span::styled("XOFF: ", Style::default().fg(Color::Gray)),
            Span::styled(
                yn(app.status.xoff),
                if app.status.xoff {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::Green)
                },
            ),
            Span::raw("   "),
            Span::styled("Breakin: ", Style::default().fg(Color::Gray)),
            Span::styled(
                yn(app.status.breakin),
                if app.status.breakin {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::Green)
                },
            ),
        ]),
        Line::from(vec![
            Span::styled("Waiting: ", Style::default().fg(Color::Gray)),
            Span::styled(
                yn(app.status.waiting),
                if app.status.waiting {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::Green)
                },
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Speed Pot: ", Style::default().fg(Color::Gray)),
            Span::styled(pot_str, Style::default().fg(Color::White)),
        ]),
    ];
    let status_widget =
        Paragraph::new(status_text).block(Block::default().borders(Borders::ALL).title(" Status "));
    frame.render_widget(status_widget, top[1]);

    // -- Echo line --
    let echo_style = Style::default().fg(Color::Green);
    let echo = Paragraph::new(Line::from(Span::styled(&app.echo_buf, echo_style)))
        .block(Block::default().borders(Borders::ALL).title(" Echo "));
    frame.render_widget(echo, outer[1]);

    // -- Input line --
    let input_border_style = if app.focus == Focus::Input {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let input = Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Yellow)),
        Span::raw(&app.input_buf),
        if app.focus == Focus::Input {
            Span::styled("_", Style::default().fg(Color::Cyan).add_modifier(Modifier::SLOW_BLINK))
        } else {
            Span::raw("")
        },
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Input ")
            .border_style(input_border_style),
    );
    frame.render_widget(input, outer[2]);

    // -- Help bar --
    let help = Paragraph::new(Line::from(vec![
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(":quit "),
        Span::styled("t", Style::default().fg(Color::Yellow)),
        Span::raw(":tune "),
        Span::styled("Tab", Style::default().fg(Color::Yellow)),
        Span::raw(":input "),
        Span::styled("\u{2191}\u{2193}", Style::default().fg(Color::Yellow)),
        Span::raw(":nav "),
        Span::styled("\u{2190}\u{2192}", Style::default().fg(Color::Yellow)),
        Span::raw(":adjust "),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(":toggle/send "),
        Span::styled("Ctrl+C", Style::default().fg(Color::Yellow)),
        Span::raw(":abort"),
    ]))
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, outer[3]);
}

// ---------------------------------------------------------------------------
// Setting adjustment helpers
// ---------------------------------------------------------------------------

enum Adjustment {
    Increment,
    Decrement,
}

fn current_pin_config(app: &App) -> PinConfig {
    let mut cfg = PinConfig::KEY_OUTPUT; // Always enable primary key output
    if app.ptt_on {
        cfg |= PinConfig::PTT_ENABLE;
    }
    if app.sidetone_on {
        cfg |= PinConfig::SIDETONE_ENABLE;
    }
    // Hang time: bits 5-4 encode 0-3
    if app.hang_time & 0x02 != 0 {
        cfg |= PinConfig::HANG_TIME_1;
    }
    if app.hang_time & 0x01 != 0 {
        cfg |= PinConfig::HANG_TIME_0;
    }
    cfg
}

async fn adjust_setting(
    app: &mut App,
    keyer: &winkey::WinKeyer,
    dir: Adjustment,
) {
    match app.selected {
        0 => {
            // Speed 5-99
            app.speed = match dir {
                Adjustment::Increment => app.speed.saturating_add(1).min(99),
                Adjustment::Decrement => app.speed.saturating_sub(1).max(5),
            };
            let _ = keyer.set_speed(app.speed).await;
        }
        1 => {
            // Weight 10-90
            app.weight = match dir {
                Adjustment::Increment => app.weight.saturating_add(1).min(90),
                Adjustment::Decrement => app.weight.saturating_sub(1).max(10),
            };
            let _ = keyer.set_weight(app.weight).await;
        }
        2 => {
            // Sidetone freq 500-4000 Hz (50 Hz steps)
            app.sidetone = match dir {
                Adjustment::Increment => app.sidetone.saturating_add(50).min(4000),
                Adjustment::Decrement => app.sidetone.saturating_sub(50).max(500),
            };
            let _ = keyer.set_sidetone(app.sidetone).await;
        }
        3 => {
            // Sidetone volume 1-4
            app.sidetone_vol = match dir {
                Adjustment::Increment => app.sidetone_vol.saturating_add(1).min(4),
                Adjustment::Decrement => app.sidetone_vol.saturating_sub(1).max(1),
            };
            let _ = keyer.set_sidetone_volume(app.sidetone_vol).await;
        }
        4 => {
            // Sidetone on/off
            app.sidetone_on = !app.sidetone_on;
            let _ = keyer.set_pin_config(current_pin_config(app)).await;
        }
        5 => {
            // Farnsworth 0-99
            app.farnsworth = match dir {
                Adjustment::Increment => app.farnsworth.saturating_add(1).min(99),
                Adjustment::Decrement => app.farnsworth.saturating_sub(1),
            };
            let _ = keyer.set_farnsworth(app.farnsworth).await;
        }
        6 => {
            // Dit/Dah ratio 33-66
            app.ratio = match dir {
                Adjustment::Increment => app.ratio.saturating_add(1).min(66),
                Adjustment::Decrement => app.ratio.saturating_sub(1).max(33),
            };
            let _ = keyer.set_ratio(app.ratio).await;
        }
        7 => {
            // Paddle mode enum
            app.paddle_mode = match dir {
                Adjustment::Increment => app.paddle_mode.next(),
                Adjustment::Decrement => app.paddle_mode.prev(),
            };
            let _ = keyer.set_paddle_mode(app.paddle_mode.to_protocol()).await;
        }
        8 => {
            // PTT on/off
            app.ptt_on = !app.ptt_on;
            let _ = keyer.set_pin_config(current_pin_config(app)).await;
        }
        9 => {
            // PTT lead-in 0-250
            app.ptt_lead_in = match dir {
                Adjustment::Increment => app.ptt_lead_in.saturating_add(1).min(250),
                Adjustment::Decrement => app.ptt_lead_in.saturating_sub(1),
            };
            let _ = keyer.set_ptt_timing(app.ptt_lead_in, app.ptt_tail).await;
        }
        10 => {
            // PTT tail 0-250
            app.ptt_tail = match dir {
                Adjustment::Increment => app.ptt_tail.saturating_add(1).min(250),
                Adjustment::Decrement => app.ptt_tail.saturating_sub(1),
            };
            let _ = keyer.set_ptt_timing(app.ptt_lead_in, app.ptt_tail).await;
        }
        11 => {
            // Hang time 0-3
            app.hang_time = match dir {
                Adjustment::Increment => (app.hang_time + 1).min(3),
                Adjustment::Decrement => app.hang_time.saturating_sub(1),
            };
            let _ = keyer.set_pin_config(current_pin_config(app)).await;
        }
        12 => {
            // Tune toggle
            app.tune = !app.tune;
            let _ = keyer.set_tune(app.tune).await;
        }
        13 => {
            // Pause toggle
            app.pause = !app.pause;
            let _ = keyer.set_pause(app.pause).await;
        }
        _ => {}
    }
}

async fn toggle_setting(app: &mut App, keyer: &winkey::WinKeyer) {
    match app.selected {
        4 => {
            app.sidetone_on = !app.sidetone_on;
            let _ = keyer.set_pin_config(current_pin_config(app)).await;
        }
        7 => {
            app.paddle_mode = app.paddle_mode.next();
            let _ = keyer.set_paddle_mode(app.paddle_mode.to_protocol()).await;
        }
        8 => {
            app.ptt_on = !app.ptt_on;
            let _ = keyer.set_pin_config(current_pin_config(app)).await;
        }
        11 => {
            // Cycle hang time forward on Enter
            app.hang_time = (app.hang_time + 1) % 4;
            let _ = keyer.set_pin_config(current_pin_config(app)).await;
        }
        12 => {
            app.tune = !app.tune;
            let _ = keyer.set_tune(app.tune).await;
        }
        13 => {
            app.pause = !app.pause;
            let _ = keyer.set_pause(app.pause).await;
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Event dispatching
// ---------------------------------------------------------------------------

async fn handle_event(
    ev: AppEvent,
    app: &mut App,
    keyer: &winkey::WinKeyer,
) {
    match ev {
        AppEvent::Tick => {}
        AppEvent::Keyer(keyer_ev) => match keyer_ev {
            KeyerEvent::StatusChanged(s) => app.status = s,
            KeyerEvent::SpeedPotChanged { wpm } => app.speed_pot = Some(wpm),
            KeyerEvent::CharacterSent(ch) => {
                app.echo_buf.push(ch);
                // Keep echo buffer from growing unbounded
                if app.echo_buf.len() > 200 {
                    let drain_to = app.echo_buf.len() - 160;
                    app.echo_buf.drain(..drain_to);
                }
            }
            KeyerEvent::PaddleBreakIn => {}
            KeyerEvent::Connected => {}
            KeyerEvent::Disconnected => app.quit = true,
        },
        AppEvent::Terminal(Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            ..
        })) => {
            // Global: Ctrl+C aborts CW
            if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
                let _ = keyer.abort().await;
                return;
            }

            match app.focus {
                Focus::Settings => match code {
                    KeyCode::Char('q') => app.quit = true,
                    KeyCode::Char('t') => {
                        app.tune = !app.tune;
                        let _ = keyer.set_tune(app.tune).await;
                    }
                    KeyCode::Tab => app.focus = Focus::Input,
                    KeyCode::Up => {
                        app.selected = app.selected.checked_sub(1).unwrap_or(NUM_SETTINGS - 1);
                    }
                    KeyCode::Down => {
                        app.selected = (app.selected + 1) % NUM_SETTINGS;
                    }
                    KeyCode::Right => adjust_setting(app, keyer, Adjustment::Increment).await,
                    KeyCode::Left => adjust_setting(app, keyer, Adjustment::Decrement).await,
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        toggle_setting(app, keyer).await;
                    }
                    _ => {}
                },
                Focus::Input => match code {
                    KeyCode::Esc => {
                        app.focus = Focus::Settings;
                    }
                    KeyCode::Tab => {
                        app.focus = Focus::Settings;
                    }
                    KeyCode::Enter => {
                        if !app.input_buf.is_empty() {
                            let text = app.input_buf.drain(..).collect::<String>();
                            let _ = keyer.send_message(&text).await;
                        }
                    }
                    KeyCode::Backspace => {
                        app.input_buf.pop();
                    }
                    KeyCode::Char(c) => {
                        app.input_buf.push(c.to_ascii_uppercase());
                    }
                    _ => {}
                },
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <port> [--speed <wpm>]", args[0]);
        eprintln!("Example: {} /dev/ttyUSB0 --speed 25", args[0]);
        std::process::exit(1);
    }

    let port = &args[1];
    let speed: u8 = args
        .iter()
        .position(|a| a == "--speed")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    let no_sidetone = args.iter().any(|a| a == "--no-sidetone");

    // Connect to keyer before entering raw mode so errors print normally
    let mut builder = WinKeyerBuilder::new(port).speed(speed);
    if no_sidetone {
        builder = builder.pin_config(PinConfig::PTT_ENABLE | PinConfig::KEY_OUTPUT);
    }
    let keyer = builder.build().await?;
    let keyer_name = keyer.info().name.clone();
    let keyer_port = keyer.info().port.clone().unwrap_or_default();

    // Setup terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(keyer_name, keyer_port, speed);
    if no_sidetone {
        app.sidetone_on = false;
    }

    // Unified event channel
    let (ev_tx, mut ev_rx) = mpsc::unbounded_channel::<AppEvent>();

    // Task 1: Terminal events
    let tx1 = ev_tx.clone();
    tokio::spawn(async move {
        let mut stream = EventStream::new();
        while let Some(Ok(event)) = stream.next().await {
            if tx1.send(AppEvent::Terminal(event)).is_err() {
                break;
            }
        }
    });

    // Task 2: Keyer events
    let tx2 = ev_tx.clone();
    let mut keyer_rx = keyer.subscribe();
    tokio::spawn(async move {
        loop {
            match keyer_rx.recv().await {
                Ok(event) => {
                    if tx2.send(AppEvent::Keyer(event)).is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    });

    // Task 3: Tick timer
    let tx3 = ev_tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        loop {
            interval.tick().await;
            if tx3.send(AppEvent::Tick).is_err() {
                break;
            }
        }
    });

    // Initial draw
    terminal.draw(|f| ui(f, &app))?;

    // Main loop
    while !app.quit {
        if let Some(ev) = ev_rx.recv().await {
            handle_event(ev, &mut app, &keyer).await;
            terminal.draw(|f| ui(f, &app))?;
        } else {
            break;
        }
    }

    // Cleanup
    let _ = keyer.close().await;
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    Ok(())
}
