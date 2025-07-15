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
    show_all_devices: bool, // Toggle to show all devices including inactive ones
}

impl App {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let gilrs = Gilrs::new().map_err(|e| format!("Failed to initialize gilrs: {}", e))?;
        
        Ok(App {
            gilrs,
            gamepads: HashMap::new(),
            running: true,
            show_all_devices: false,
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

        // Filter to only show active gamepads (with recent activity) unless showing all devices
        let displayed_gamepads: Vec<_> = if self.show_all_devices {
            // Show all connected gamepads
            self.gamepads.iter().filter(|(_, gamepad)| gamepad.connected).collect()
        } else {
            // Show only active gamepads
            self.gamepads.iter()
                .filter(|(_, gamepad)| {
                    // Show if connected and has recent activity (within last 30 seconds)
                    gamepad.connected && 
                    gamepad.last_activity
                        .map(|last| last.elapsed() < Duration::from_secs(30))
                        .unwrap_or(false) &&
                    // Also check if it has any axis values (indicating it's a real controller)
                    (!gamepad.axes.is_empty() || !gamepad.buttons.is_empty())
                })
                .collect()
        };

        // Header with enhanced styling
        let header_text = if displayed_gamepads.is_empty() {
            if self.show_all_devices {
                "🎮 Gamepad Visualizer - Press 'q' to quit, 'd' to toggle debug | No gamepads detected"
            } else {
                "🎮 Gamepad Visualizer - Press 'q' to quit, 'd' to show all devices | No active gamepads"
            }
        } else {
            if self.show_all_devices {
                &format!("🎮 Gamepad Visualizer - Press 'q' to quit, 'd' to hide inactive | {} gamepad(s) [DEBUG MODE]", displayed_gamepads.len())
            } else {
                &format!("🎮 Gamepad Visualizer - Press 'q' to quit, 'd' to show all devices | {} active gamepad(s)", displayed_gamepads.len())
            }
        };
        let header = Paragraph::new(header_text)
            .block(Block::default().borders(Borders::ALL))
            .style(Style::default().fg(Color::Cyan));
        frame.render_widget(header, chunks[0]);

        if displayed_gamepads.is_empty() {
            let total_connected = self.gamepads.values().filter(|g| g.connected).count();
            let no_gamepad = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled("🕹️  No active gamepads detected", Style::default().fg(Color::Yellow))),
                Line::from(""),
                if total_connected > 0 && !self.show_all_devices {
                    Line::from(format!("({} HID device(s) connected but inactive - press 'd' to show all)", total_connected))
                } else if total_connected > 0 {
                    Line::from(format!("({} device(s) connected)", total_connected))
                } else {
                    Line::from("No gamepads connected")
                },
                Line::from(""),
                Line::from("Please connect a gamepad and try:"),
                Line::from("• Moving the analog sticks"),
                Line::from("• Pressing buttons"),
                Line::from("• Using triggers or motion controls"),
                Line::from(""),
                Line::from(Span::styled("Controls:", Style::default().fg(Color::Gray))),
                Line::from("• 'd' - Toggle debug mode (show all devices)"),
                Line::from("• 'q' - Quit application"),
                if !self.show_all_devices {
                    Line::from("• Only controllers with recent activity are shown")
                } else {
                    Line::from("• Debug mode: showing all connected devices")
                },
            ])
                .block(Block::default().borders(Borders::ALL).title("🎯 Status"))
                .style(Style::default());
            frame.render_widget(no_gamepad, chunks[1]);
            return;
        }

        // Split the main area for multiple displayed gamepads
        let gamepad_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Min(0); displayed_gamepads.len()])
            .split(chunks[1]);

        for (i, (id, gamepad)) in displayed_gamepads.iter().enumerate() {
            if i >= gamepad_chunks.len() {
                break;
            }

            self.draw_gamepad(frame, gamepad_chunks[i], **id, gamepad);
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

        // Title with gamepad info and activity indicator
        let activity_indicator = if let Some(last_activity) = gamepad.last_activity {
            let seconds_ago = last_activity.elapsed().as_secs();
            if seconds_ago < 5 {
                "🟢 ACTIVE"
            } else if seconds_ago < 15 {
                "🟡 RECENT"
            } else {
                "🟠 IDLE"
            }
        } else {
            "⚫ INACTIVE"
        };
        
        let title = format!("🎮 {} ({}) {} 🎮", gamepad.name, status_text, activity_indicator);
        let title_widget = Paragraph::new(title)
            .style(Style::default().fg(status_color))
            .block(Block::default().borders(Borders::NONE));
        frame.render_widget(title_widget, main_chunks[0]);

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(35),  // Left side - Joysticks and motion
                Constraint::Percentage(30),  // Middle - 3D Gimbal visualization
                Constraint::Percentage(20),  // Buttons
                Constraint::Percentage(15),  // Right side - All axes
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

        // 3D Gimbal Visualization
        let gimbal_canvas = Canvas::default()
            .block(Block::default().borders(Borders::ALL).title("🎯 EPL 3-Axis Gimbal"))
            .paint(|ctx| {
                // Get joystick values for gimbal control
                let yaw = gamepad.axes.get(&Axis::LeftStickX).copied().unwrap_or(0.0);   // Left stick X -> Yaw
                let pitch = gamepad.axes.get(&Axis::LeftStickY).copied().unwrap_or(0.0); // Left stick Y -> Pitch  
                let roll = gamepad.axes.get(&Axis::RightStickX).copied().unwrap_or(0.0); // Right stick X -> Roll

                // Convert joystick values to angles (in radians for calculation)
                let yaw_angle = (yaw as f64) * std::f64::consts::PI / 4.0;    // ±45 degrees
                let pitch_angle = (pitch as f64) * std::f64::consts::PI / 4.0; // ±45 degrees
                let roll_angle = (roll as f64) * std::f64::consts::PI / 4.0;   // ±45 degrees

                // Gimbal dimensions
                let outer_radius = 45.0;  // Yaw ring (outermost)
                let middle_radius = 35.0; // Pitch ring
                let inner_radius = 25.0;  // Roll platform

                // Draw outer yaw ring (rotates around Z-axis)
                let yaw_points = 16;
                for i in 0..yaw_points {
                    let angle1 = (i as f64 * 2.0 * std::f64::consts::PI / yaw_points as f64) + yaw_angle;
                    let angle2 = ((i + 1) as f64 * 2.0 * std::f64::consts::PI / yaw_points as f64) + yaw_angle;
                    
                    let x1 = outer_radius * angle1.cos();
                    let y1 = outer_radius * angle1.sin();
                    let x2 = outer_radius * angle2.cos();
                    let y2 = outer_radius * angle2.sin();
                    
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1, y1, x2, y2,
                        color: Color::Cyan,
                    });
                }

                // Draw middle pitch ring (rotates around X-axis, but shown as vertical ellipse)
                let pitch_points = 12;
                for i in 0..pitch_points {
                    let angle1 = i as f64 * 2.0 * std::f64::consts::PI / pitch_points as f64;
                    let angle2 = (i + 1) as f64 * 2.0 * std::f64::consts::PI / pitch_points as f64;
                    
                    // Apply pitch rotation (simplified 2D projection)
                    let x1 = middle_radius * angle1.cos();
                    let y1 = (middle_radius * angle1.sin()) * pitch_angle.cos() - (middle_radius * 0.3) * pitch_angle.sin();
                    let x2 = middle_radius * angle2.cos();
                    let y2 = (middle_radius * angle2.sin()) * pitch_angle.cos() - (middle_radius * 0.3) * pitch_angle.sin();
                    
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1, y1, x2, y2,
                        color: Color::Yellow,
                    });
                }

                // Draw inner roll platform/gimbal (rotates around Y-axis)
                let platform_points = 8;
                for i in 0..platform_points {
                    let angle1 = (i as f64 * 2.0 * std::f64::consts::PI / platform_points as f64) + roll_angle;
                    let angle2 = ((i + 1) as f64 * 2.0 * std::f64::consts::PI / platform_points as f64) + roll_angle;
                    
                    let x1 = inner_radius * angle1.cos() * 0.7; // Flatten to show roll
                    let y1 = inner_radius * angle1.sin();
                    let x2 = inner_radius * angle2.cos() * 0.7;
                    let y2 = inner_radius * angle2.sin();
                    
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1, y1, x2, y2,
                        color: Color::LightRed,
                    });
                }

                // Draw gimbal center platform (the payload/camera mount)
                let platform_size = 15.0;
                let cos_roll = roll_angle.cos();
                let sin_roll = roll_angle.sin();
                
                // Platform corners (rotated by roll)
                let corners = [
                    (-platform_size, -platform_size * 0.5),
                    (platform_size, -platform_size * 0.5),
                    (platform_size, platform_size * 0.5),
                    (-platform_size, platform_size * 0.5),
                ];
                
                for i in 0..4 {
                    let (x1, y1) = corners[i];
                    let (x2, y2) = corners[(i + 1) % 4];
                    
                    // Apply roll rotation
                    let rx1 = x1 * cos_roll - y1 * sin_roll;
                    let ry1 = x1 * sin_roll + y1 * cos_roll + pitch_angle * 10.0; // Add pitch offset
                    let rx2 = x2 * cos_roll - y2 * sin_roll;
                    let ry2 = x2 * sin_roll + y2 * cos_roll + pitch_angle * 10.0;
                    
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: rx1, y1: ry1, x2: rx2, y2: ry2,
                        color: Color::LightGreen,
                    });
                }

                // Draw center dot to show platform orientation
                ctx.draw(&ratatui::widgets::canvas::Circle {
                    x: 0.0,
                    y: pitch_angle * 10.0,
                    radius: 3.0,
                    color: Color::White,
                });

                // Draw axis indicators
                // X-axis (roll axis) - Red
                ctx.draw(&ratatui::widgets::canvas::Line {
                    x1: -60.0, y1: -60.0, x2: -45.0, y2: -60.0,
                    color: Color::Red,
                });
                // Y-axis (pitch axis) - Green  
                ctx.draw(&ratatui::widgets::canvas::Line {
                    x1: -60.0, y1: -60.0, x2: -60.0, y2: -45.0,
                    color: Color::Green,
                });
                // Z-axis (yaw axis) - Blue (diagonal to show depth)
                ctx.draw(&ratatui::widgets::canvas::Line {
                    x1: -60.0, y1: -60.0, x2: -50.0, y2: -50.0,
                    color: Color::Blue,
                });
            })
            .x_bounds([-80.0, 80.0])
            .y_bounds([-80.0, 80.0]);
        frame.render_widget(gimbal_canvas, chunks[1]);

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
        frame.render_widget(buttons_list, chunks[2]);

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
            .split(chunks[3]);

        let axis_header = Paragraph::new("📊 All Axes")
            .block(Block::default().borders(Borders::ALL))
            .style(Style::default().fg(Color::Cyan));
        frame.render_widget(axis_header, axis_chunks[0]);

        // Create a comprehensive list of all detected axes
        let mut axis_display: Vec<(String, f32, Color)> = Vec::new();
        
        // Add gimbal angle information first
        let left_x = gamepad.axes.get(&Axis::LeftStickX).copied().unwrap_or(0.0);
        let left_y = gamepad.axes.get(&Axis::LeftStickY).copied().unwrap_or(0.0);
        let right_x = gamepad.axes.get(&Axis::RightStickX).copied().unwrap_or(0.0);
        
        axis_display.push(("🎯 Gimbal Yaw".to_string(), left_x, Color::Cyan));
        axis_display.push(("🎯 Gimbal Pitch".to_string(), left_y, Color::Yellow));
        axis_display.push(("🎯 Gimbal Roll".to_string(), right_x, Color::LightRed));
        
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
                    KeyCode::Char('d') | KeyCode::Char('D') => {
                        app.show_all_devices = !app.show_all_devices;
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
