use gilrs::{Gilrs, Event, Axis, Button};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
    widgets::canvas::Canvas,
    Frame, Terminal,
};
use crossterm::{
    event::{self, Event as CrosstermEvent, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    collections::HashMap,
    io::stdout,
    time::{Duration, Instant},
};

#[derive(Default)]
struct GamepadState {
    name: String,
    connected: bool,
    axes: HashMap<Axis, f32>,
    buttons: HashMap<Button, bool>,
    last_activity: Option<Instant>,
}

struct App {
    gilrs: Gilrs,
    gamepads: HashMap<gilrs::GamepadId, GamepadState>,
    running: bool,
}

impl App {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let gilrs = Gilrs::new().map_err(|e| format!("Failed to initialize gilrs: {}", e))?;
        
        Ok(App {
            gilrs,
            gamepads: HashMap::new(),
            running: true,
        })
    }

    fn update(&mut self) {
        // Process all pending events
        while let Some(Event { id, event, .. }) = self.gilrs.next_event() {
            let gamepad_state = self.gamepads.entry(id).or_insert_with(|| GamepadState {
                name: self.gilrs.gamepad(id).name().to_string(),
                connected: true,
                axes: HashMap::new(),
                buttons: HashMap::new(),
                last_activity: Some(Instant::now()),
            });

            gamepad_state.last_activity = Some(Instant::now());

            match event {
                gilrs::EventType::ButtonPressed(button, _) => {
                    gamepad_state.buttons.insert(button, true);
                },
                gilrs::EventType::ButtonReleased(button, _) => {
                    gamepad_state.buttons.insert(button, false);
                },
                gilrs::EventType::AxisChanged(axis, value, _) => {
                    gamepad_state.axes.insert(axis, value);
                },
                gilrs::EventType::Connected => {
                    gamepad_state.connected = true;
                    gamepad_state.name = self.gilrs.gamepad(id).name().to_string();
                },
                gilrs::EventType::Disconnected => {
                    gamepad_state.connected = false;
                },
                _ => {}
            }
        }
    }

    fn draw(&self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(frame.area());

        // Header with enhanced styling
        let header_text = if self.gamepads.is_empty() {
            "🎮 Gamepad Visualizer - Press 'q' to quit | No gamepads detected"
        } else {
            &format!("🎮 Gamepad Visualizer - Press 'q' to quit | {} gamepad(s) connected", self.gamepads.len())
        };
        let header = Paragraph::new(header_text)
            .block(Block::default().borders(Borders::ALL))
            .style(Style::default().fg(Color::Cyan));
        frame.render_widget(header, chunks[0]);

        if self.gamepads.is_empty() {
            let no_gamepad = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled("🕹️  No gamepads connected", Style::default().fg(Color::Yellow))),
                Line::from(""),
                Line::from("Please connect a gamepad and try:"),
                Line::from("• Moving the analog sticks"),
                Line::from("• Pressing buttons"),
                Line::from("• Using triggers or motion controls"),
                Line::from(""),
                Line::from(Span::styled("Supports all standard gamepad axes including:", Style::default().fg(Color::Gray))),
                Line::from("• Left/Right sticks (X, Y)"),
                Line::from("• Triggers (Z axes)"),
                Line::from("• Motion sensors (Tx, Ty, Tz, Rx, Ry, Rz)"),
                Line::from("• D-Pad and any custom axes"),
            ])
                .block(Block::default().borders(Borders::ALL).title("🎯 Status"))
                .style(Style::default());
            frame.render_widget(no_gamepad, chunks[1]);
            return;
        }

        // Split the main area for multiple gamepads
        let gamepad_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Min(0); self.gamepads.len()])
            .split(chunks[1]);

        for (i, (id, gamepad)) in self.gamepads.iter().enumerate() {
            if i >= gamepad_chunks.len() {
                break;
            }

            self.draw_gamepad(frame, gamepad_chunks[i], *id, gamepad);
        }
    }

    fn draw_gamepad(&self, frame: &mut Frame, area: Rect, _id: gilrs::GamepadId, gamepad: &GamepadState) {
        let status_color = if gamepad.connected { Color::Green } else { Color::Red };
        let status_text = if gamepad.connected { "Connected" } else { "Disconnected" };

        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),    // Title
                Constraint::Min(15),      // Main content
            ])
            .split(area);

        // Title with gamepad info
        let title = format!("🎮 {} ({}) 🎮", gamepad.name, status_text);
        let title_widget = Paragraph::new(title)
            .style(Style::default().fg(status_color))
            .block(Block::default().borders(Borders::NONE));
        frame.render_widget(title_widget, main_chunks[0]);

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),  // Left side - Joysticks and motion
                Constraint::Percentage(25),  // Middle - Buttons
                Constraint::Percentage(25),  // Right side - All axes
            ])
            .split(main_chunks[1]);

        // Left side - Joysticks and motion sensors
        let left_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(70),  // Main joysticks
                Constraint::Percentage(30),  // Motion sensors (Tx, Ty, Tz, Rx, Ry, Rz)
            ])
            .split(chunks[0]);

        // Main joystick visualization
        let canvas = Canvas::default()
            .block(Block::default().borders(Borders::ALL).title("🕹️  Analog Sticks"))
            .paint(|ctx| {
                // Left joystick (typically LeftStickX and LeftStickY)
                let left_x = gamepad.axes.get(&Axis::LeftStickX).copied().unwrap_or(0.0);
                let left_y = gamepad.axes.get(&Axis::LeftStickY).copied().unwrap_or(0.0);
                
                // Right joystick (typically RightStickX and RightStickY)
                let right_x = gamepad.axes.get(&Axis::RightStickX).copied().unwrap_or(0.0);
                let right_y = gamepad.axes.get(&Axis::RightStickY).copied().unwrap_or(0.0);

                // Draw joystick boundaries with better styling
                ctx.draw(&ratatui::widgets::canvas::Circle {
                    x: -50.0,
                    y: 0.0,
                    radius: 40.0,
                    color: Color::White,
                });
                ctx.draw(&ratatui::widgets::canvas::Circle {
                    x: 50.0,
                    y: 0.0,
                    radius: 40.0,
                    color: Color::White,
                });

                // Draw inner circles for reference
                ctx.draw(&ratatui::widgets::canvas::Circle {
                    x: -50.0,
                    y: 0.0,
                    radius: 20.0,
                    color: Color::DarkGray,
                });
                ctx.draw(&ratatui::widgets::canvas::Circle {
                    x: 50.0,
                    y: 0.0,
                    radius: 20.0,
                    color: Color::DarkGray,
                });

                // Draw joystick positions with better colors and size
                ctx.draw(&ratatui::widgets::canvas::Circle {
                    x: -50.0 + (left_x * 35.0) as f64,
                    y: -(left_y * 35.0) as f64,
                    radius: 6.0,
                    color: Color::LightRed,
                });
                ctx.draw(&ratatui::widgets::canvas::Circle {
                    x: 50.0 + (right_x * 35.0) as f64,
                    y: -(right_y * 35.0) as f64,
                    radius: 6.0,
                    color: Color::LightBlue,
                });

                // Draw center crosses with better styling
                ctx.draw(&ratatui::widgets::canvas::Line {
                    x1: -65.0,
                    y1: 0.0,
                    x2: -35.0,
                    y2: 0.0,
                    color: Color::Gray,
                });
                ctx.draw(&ratatui::widgets::canvas::Line {
                    x1: -50.0,
                    y1: -15.0,
                    x2: -50.0,
                    y2: 15.0,
                    color: Color::Gray,
                });
                ctx.draw(&ratatui::widgets::canvas::Line {
                    x1: 35.0,
                    y1: 0.0,
                    x2: 65.0,
                    y2: 0.0,
                    color: Color::Gray,
                });
                ctx.draw(&ratatui::widgets::canvas::Line {
                    x1: 50.0,
                    y1: -15.0,
                    x2: 50.0,
                    y2: 15.0,
                    color: Color::Gray,
                });

                // Add labels
                // Note: Canvas doesn't support text directly, but the positioning shows left/right
            })
            .x_bounds([-100.0, 100.0])
            .y_bounds([-50.0, 50.0]);
        frame.render_widget(canvas, left_chunks[0]);

        // Motion sensors visualization
        let motion_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),  // Translation (Tx, Ty, Tz)
                Constraint::Percentage(50),  // Rotation (Rx, Ry, Rz)
            ])
            .split(left_chunks[1]);

        // Translation sensors (Tx, Ty, Tz)
        let translation_canvas = Canvas::default()
            .block(Block::default().borders(Borders::ALL).title("📍 Motion Sensors"))
            .paint(|ctx| {
                // Draw 3D coordinate system
                ctx.draw(&ratatui::widgets::canvas::Line {
                    x1: 0.0, y1: 0.0, x2: 30.0, y2: 0.0, color: Color::Red,  // X axis
                });
                ctx.draw(&ratatui::widgets::canvas::Line {
                    x1: 0.0, y1: 0.0, x2: 0.0, y2: 30.0, color: Color::Green,  // Y axis  
                });
                ctx.draw(&ratatui::widgets::canvas::Line {
                    x1: 0.0, y1: 0.0, x2: -15.0, y2: -15.0, color: Color::Blue,  // Z axis (perspective)
                });

                // Find and display any motion sensor values
                let mut motion_detected = false;
                for (axis, &value) in &gamepad.axes {
                    let axis_name = format!("{:?}", axis);
                    // Look for motion sensor axes (not standard gamepad axes)
                    if !matches!(axis, Axis::LeftStickX | Axis::LeftStickY | Axis::RightStickX | 
                                      Axis::RightStickY | Axis::LeftZ | Axis::RightZ | 
                                      Axis::DPadX | Axis::DPadY) {
                        // Draw motion indicator
                        let x_pos = (value * 25.0).clamp(-35.0, 35.0);
                        ctx.draw(&ratatui::widgets::canvas::Circle {
                            x: x_pos as f64,
                            y: 0.0,
                            radius: 3.0,
                            color: if axis_name.contains('T') { Color::Yellow } else { Color::Magenta },
                        });
                        motion_detected = true;
                    }
                }

                if !motion_detected {
                    // Show a neutral indicator
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: 0.0, y: 0.0, radius: 2.0, color: Color::DarkGray,
                    });
                }
            })
            .x_bounds([-40.0, 40.0])
            .y_bounds([-40.0, 40.0]);
        frame.render_widget(translation_canvas, motion_chunks[0]);

        // Rotation sensors (Rx, Ry, Rz)
        let rotation_canvas = Canvas::default()
            .block(Block::default().borders(Borders::ALL).title("🔄 Rotation (R)"))
            .paint(|ctx| {
                // Draw rotation indicators - circular arcs would be ideal but we'll use lines
                ctx.draw(&ratatui::widgets::canvas::Circle {
                    x: 0.0, y: 0.0, radius: 30.0, color: Color::White,
                });
                ctx.draw(&ratatui::widgets::canvas::Circle {
                    x: 0.0, y: 0.0, radius: 15.0, color: Color::DarkGray,
                });
                
                // Draw rotation indicator
                ctx.draw(&ratatui::widgets::canvas::Circle {
                    x: 0.0, y: 0.0, radius: 5.0, color: Color::Magenta,
                });
            })
            .x_bounds([-40.0, 40.0])
            .y_bounds([-40.0, 40.0]);
        frame.render_widget(rotation_canvas, motion_chunks[1]);

        // Button status with better styling
        let button_items: Vec<ListItem> = gamepad.buttons.iter()
            .map(|(button, pressed)| {
                let (style, status) = if *pressed { 
                    (Style::default().fg(Color::LightGreen), "🟢") 
                } else { 
                    (Style::default().fg(Color::DarkGray), "⚫") 
                };
                ListItem::new(Line::from(vec![
                    Span::styled(status, style),
                    Span::raw(format!(" {:?}", button)),
                ]))
            })
            .collect();

        let buttons_list = List::new(button_items)
            .block(Block::default().borders(Borders::ALL).title("🎯 Buttons"))
            .style(Style::default());
        frame.render_widget(buttons_list, chunks[1]);

        // Enhanced axis values display - show ALL axes
        let all_possible_axes = [
            ("Left Stick", vec![Axis::LeftStickX, Axis::LeftStickY]),
            ("Right Stick", vec![Axis::RightStickX, Axis::RightStickY]),
            ("Triggers", vec![Axis::LeftZ, Axis::RightZ]),
            ("D-Pad", vec![Axis::DPadX, Axis::DPadY]),
        ];

        let axis_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(0),    // Axis list
            ])
            .split(chunks[2]);

        let axis_header = Paragraph::new("📊 All Axes")
            .block(Block::default().borders(Borders::ALL))
            .style(Style::default().fg(Color::Cyan));
        frame.render_widget(axis_header, axis_chunks[0]);

        // Create a comprehensive list of all detected axes
        let mut axis_display: Vec<(String, f32, Color)> = Vec::new();
        
        // Add known axes with nice names and colors
        for (_category, axes) in all_possible_axes {
            for axis in axes {
                if let Some(&value) = gamepad.axes.get(&axis) {
                    let color = match axis {
                        Axis::LeftStickX | Axis::LeftStickY => Color::LightRed,
                        Axis::RightStickX | Axis::RightStickY => Color::LightBlue,
                        Axis::LeftZ | Axis::RightZ => Color::Yellow,
                        Axis::DPadX | Axis::DPadY => Color::LightCyan,
                        _ => Color::White,
                    };
                    axis_display.push((format!("{:?}", axis), value, color));
                }
            }
        }

        // Add any unknown/additional axes (like Tx, Ty, Tz, Rx, Ry, Rz)
        for (axis, &value) in &gamepad.axes {
            let axis_name = format!("{:?}", axis);
            if !axis_display.iter().any(|(name, _, _)| name == &axis_name) {
                let color = if axis_name.contains('T') {
                    Color::LightMagenta  // Translation axes
                } else if axis_name.contains('R') {
                    Color::LightGreen    // Rotation axes
                } else {
                    Color::Gray          // Other unknown axes
                };
                axis_display.push((axis_name, value, color));
            }
        }

        // Sort axes for consistent display
        axis_display.sort_by(|a, b| a.0.cmp(&b.0));

        // Create vertical layout for all axes
        let axis_count = axis_display.len().min(15); // Limit to prevent overflow
        if axis_count > 0 {
            let gauge_constraints = vec![Constraint::Length(3); axis_count];
            let gauge_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(gauge_constraints)
                .split(axis_chunks[1]);

            for (i, (axis_name, value, color)) in axis_display.iter().take(axis_count).enumerate() {
                let percentage = ((value + 1.0) * 50.0).clamp(0.0, 100.0) as u16;
                
                let gauge = Gauge::default()
                    .block(Block::default().title(axis_name.clone()))
                    .gauge_style(Style::default().fg(*color))
                    .percent(percentage)
                    .label(format!("{:.3}", value));
                frame.render_widget(gauge, gauge_chunks[i]);
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new()?;

    // Main loop
    let tick_rate = Duration::from_millis(16); // ~60 FPS
    let mut last_tick = Instant::now();

    while app.running {
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let CrosstermEvent::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        app.running = false;
                    }
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.update();
            last_tick = Instant::now();
        }

        terminal.draw(|f| app.draw(f))?;
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
