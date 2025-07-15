use gilrs::{Gilrs, Event, Axis, Button};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
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

        // Full-screen EPL Gimbal Visualization (Maximum Resolution)
        let gimbal_canvas = Canvas::default()
            .block(Block::default().borders(Borders::ALL).title("🎯 EPL Parallel Plate Gimbal - High Resolution"))
            .paint(|ctx| {
                // Get SpaceMouse/joystick values for gimbal control
                let pitch = gamepad.axes.get(&Axis::LeftStickY).copied().unwrap_or(0.0);  // Tilt forward/back
                let roll = gamepad.axes.get(&Axis::LeftStickX).copied().unwrap_or(0.0);   // Tilt left/right
                
                // Check for 3D SpaceMouse axes (Z-axis for up/down movement)
                let z_lift = gamepad.axes.get(&Axis::LeftZ).copied()
                    .or_else(|| gamepad.axes.get(&Axis::RightZ).copied())
                    .unwrap_or(0.0);  // Up/down movement
                
                // Also check for any Tz axis (translation Z) from SpaceMouse
                let z_translation = gamepad.axes.iter()
                    .find(|(axis, _)| format!("{:?}", axis).contains("Tz"))
                    .map(|(_, &value)| value)
                    .unwrap_or(0.0);
                
                // Use the stronger Z signal
                let z_movement = if z_translation.abs() > z_lift.abs() { z_translation } else { z_lift };

                // Convert SpaceMouse values to realistic gimbal movement
                let pitch_angle = (pitch * 25.0) as f64;  // ±25 degrees max tilt (more dramatic for visibility)
                let roll_angle = (roll * 25.0) as f64;    // ±25 degrees max tilt
                let base_lift = (z_movement * 20.0) as f64;  // ±20mm vertical movement

                // Platform dimensions - Much larger for maximum resolution
                let platform_radius = 120.0;  // Double size for high resolution
                let base_height = -60.0;
                let nominal_height = 30.0 + base_lift;  // Overall height adjustment

                // Draw base platform (fixed lower plate) - always stationary - Higher detail
                let base_points = 24;  // Much more segments for ultra-smooth circle
                for i in 0..base_points {
                    let angle1 = i as f64 * 2.0 * std::f64::consts::PI / base_points as f64;
                    let angle2 = (i + 1) as f64 * 2.0 * std::f64::consts::PI / base_points as f64;
                    
                    let x1 = platform_radius * angle1.cos();
                    let y1 = platform_radius * angle1.sin();
                    let x2 = platform_radius * angle2.cos();
                    let y2 = platform_radius * angle2.sin();
                    
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1, y1, x2, y2,
                        color: Color::DarkGray,
                    });
                }

                // Draw multiple inner reference circles for scale
                for ring in 1..4 {
                    let ring_points = 16;
                    let ring_radius = platform_radius * (ring as f64 * 0.25);
                    for i in 0..ring_points {
                        let angle1 = i as f64 * 2.0 * std::f64::consts::PI / ring_points as f64;
                        let angle2 = (i + 1) as f64 * 2.0 * std::f64::consts::PI / ring_points as f64;
                        
                        let x1 = ring_radius * angle1.cos();
                        let x2 = ring_radius * angle2.cos();
                        
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1, y1: base_height, x2, y2: base_height,
                            color: if ring == 3 { Color::Gray } else { Color::DarkGray },
                        });
                    }
                }

                // Fixed scissor lift positions (NO rotation - these are physical mounts)
                let scissor_positions: [(f64, f64); 6] = [
                    (0.0, platform_radius),      // Front (Y+)
                    (60.0, platform_radius),     // Front-right
                    (120.0, platform_radius),    // Back-right
                    (180.0, platform_radius),    // Back (Y-)
                    (240.0, platform_radius),    // Back-left
                    (300.0, platform_radius),    // Front-left
                ];

                let mut upper_plate_points = Vec::new();

                for (angle_deg, radius) in scissor_positions.iter() {
                    let angle_rad = angle_deg.to_radians();
                    
                    // Fixed position on base (no rotation)
                    let base_x = radius * angle_rad.cos();
                    let base_y = radius * angle_rad.sin();
                    
                    // Calculate scissor extension based on desired tilt
                    // Each scissor extends/retracts to achieve the plate angle
                    let pitch_effect = (base_y / platform_radius) * pitch_angle.to_radians() * platform_radius;
                    let roll_effect = (base_x / platform_radius) * roll_angle.to_radians() * platform_radius;
                    
                    // Final height for this scissor lift
                    let scissor_height = nominal_height + pitch_effect + roll_effect;
                    
                    // Upper plate connection point (same X,Y as base for parallel linkage)
                    upper_plate_points.push((base_x, base_y, scissor_height));
                    
                    // Draw enhanced scissor lift mechanism - More realistic based on photos
                    let extension = scissor_height - nominal_height;
                    let lift_color = if extension > 6.0 {
                        Color::LightGreen  // Extended
                    } else if extension < -6.0 {
                        Color::LightRed    // Retracted
                    } else {
                        Color::Yellow      // Neutral
                    };
                    
                    // Draw main scissor lift structure - thicker for high resolution
                    for offset in [-1.0, 0.0, 1.0] {
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1: base_x + offset,
                            y1: base_height,
                            x2: base_x + offset,
                            y2: scissor_height,
                            color: lift_color,
                        });
                    }
                    
                    // Draw stepper motor housing at base (much larger for visibility)
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: base_x,
                        y: base_height - 8.0,
                        radius: 6.0,
                        color: Color::Blue,
                    });
                    
                    // Draw motor mounting bracket
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: base_x - 4.0,
                        y1: base_height - 8.0,
                        x2: base_x + 4.0,
                        y2: base_height - 8.0,
                        color: Color::DarkGray,
                    });
                    
                    // Draw realistic scissor mechanism with multiple joints
                    let num_joints = 5;  // More joints for realistic mechanism
                    for j in 1..num_joints {
                        let joint_height = base_height + (scissor_height - base_height) * j as f64 / num_joints as f64;
                        
                        // Draw joint circles
                        ctx.draw(&ratatui::widgets::canvas::Circle {
                            x: base_x,
                            y: joint_height,
                            radius: 2.5,
                            color: Color::Gray,
                        });
                        
                        // Draw joint pins
                        ctx.draw(&ratatui::widgets::canvas::Circle {
                            x: base_x,
                            y: joint_height,
                            radius: 1.0,
                            color: Color::White,
                        });
                    }
                    
                    // Draw scissor cross-bracing - more detailed like in the photos
                    let support_offset = 4.0;  // Larger offset for better visibility
                    let mid_height = (base_height + scissor_height) / 2.0;
                    
                    // X-pattern scissor supports
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: base_x - support_offset,
                        y1: base_height + 5.0,
                        x2: base_x + support_offset,
                        y2: scissor_height - 5.0,
                        color: Color::Gray,
                    });
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: base_x + support_offset,
                        y1: base_height + 5.0,
                        x2: base_x - support_offset,
                        y2: scissor_height - 5.0,
                        color: Color::Gray,
                    });
                    
                    // Additional structural elements
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: base_x - 2.0,
                        y1: mid_height,
                        x2: base_x + 2.0,
                        y2: mid_height,
                        color: Color::DarkGray,
                    });
                }

                // Draw upper platform (tilted based on scissor heights)
                for i in 0..upper_plate_points.len() {
                    let (x1, _y1, h1) = upper_plate_points[i];
                    let (x2, _y2, h2) = upper_plate_points[(i + 1) % upper_plate_points.len()];
                    
                    // Draw upper plate edge using actual 3D coordinates
                    let avg_height = (h1 + h2) / 2.0;
                    let brightness = ((avg_height - (nominal_height - 8.0)) / 20.0).clamp(0.0, 1.0);
                    
                    let line_color = if brightness > 0.8 {
                        Color::White
                    } else if brightness > 0.5 {
                        Color::Gray
                    } else if brightness > 0.2 {
                        Color::DarkGray
                    } else {
                        Color::Black
                    };
                    
                    // Draw the edge of the upper plate using actual coordinates
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1, y1: h1, x2, y2: h2,
                        color: line_color,
                    });
                    
                    // Draw connection points where upper plate meets scissor lifts
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: x1,
                        y: h1,
                        radius: 2.0,
                        color: Color::LightBlue,
                    });
                }

                // Draw center payload mount on upper plate (much larger and more detailed)
                let center_height = nominal_height + 
                    (pitch_angle.to_radians() * 0.0) +  // Center doesn't move much for small tilts
                    (roll_angle.to_radians() * 0.0);
                    
                // Main payload mounting ring
                ctx.draw(&ratatui::widgets::canvas::Circle {
                    x: 0.0,
                    y: center_height,
                    radius: 15.0,  // Much larger for high resolution
                    color: Color::LightCyan,
                });
                
                // Inner mounting ring
                ctx.draw(&ratatui::widgets::canvas::Circle {
                    x: 0.0,
                    y: center_height,
                    radius: 10.0,
                    color: Color::Cyan,
                });
                
                // Draw payload mounting bolt holes (8 holes around circumference)
                let mount_radius = 12.0;
                for i in 0..8 {
                    let angle = i as f64 * std::f64::consts::PI / 4.0;
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: mount_radius * angle.cos(),
                        y: center_height + mount_radius * angle.sin(),
                        radius: 2.0,
                        color: Color::DarkGray,
                    });
                }

                // Draw enhanced tilt visualization lines (much more prominent)
                let tilt_line_length = platform_radius * 0.9;
                
                // Roll tilt line (left-right axis) - multiple lines for thickness
                let roll_tilt_height = roll_angle.to_radians() * tilt_line_length * 0.3;
                for offset in [-2.0, -1.0, 0.0, 1.0, 2.0] {
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: -tilt_line_length,
                        y1: center_height - roll_tilt_height + offset,
                        x2: tilt_line_length,
                        y2: center_height + roll_tilt_height + offset,
                        color: Color::Magenta,
                    });
                }
                
                // Pitch tilt line (forward-back axis)
                let pitch_tilt_height = pitch_angle.to_radians() * tilt_line_length * 0.3;
                for offset in [-2.0, -1.0, 0.0, 1.0, 2.0] {
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: 0.0 - pitch_tilt_height + offset,
                        y1: center_height - tilt_line_length,
                        x2: 0.0 + pitch_tilt_height + offset,
                        y2: center_height + tilt_line_length,
                        color: Color::Cyan,
                    });
                }

                // Draw enhanced coordinate system reference 
                let coord_x = -140.0;
                let coord_y = -80.0;
                ctx.draw(&ratatui::widgets::canvas::Line {
                    x1: coord_x, y1: coord_y, x2: coord_x + 30.0, y2: coord_y,
                    color: Color::Red,  // X-axis (Roll)
                });
                ctx.draw(&ratatui::widgets::canvas::Line {
                    x1: coord_x, y1: coord_y, x2: coord_x, y2: coord_y + 30.0,
                    color: Color::Green,  // Y-axis (Pitch)
                });
                ctx.draw(&ratatui::widgets::canvas::Line {
                    x1: coord_x, y1: coord_y, x2: coord_x + 15.0, y2: coord_y + 15.0,
                    color: Color::Blue,  // Z-axis (Height)
                });

                // Enhanced status indicators - much larger and more visible
                let tilt_magnitude = (pitch_angle.powi(2) + roll_angle.powi(2)).sqrt();
                if tilt_magnitude > 2.0 {
                    // Tilt warning indicator
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: 140.0,
                        y: 80.0,
                        radius: 8.0,
                        color: Color::Red,
                    });
                    
                    // Draw angle magnitude as visual bar
                    let bar_length = (tilt_magnitude * 3.0).min(30.0);
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: 140.0 - bar_length / 2.0,
                        y1: 65.0,
                        x2: 140.0 + bar_length / 2.0,
                        y2: 65.0,
                        color: Color::Red,
                    });
                }
                
                if base_lift.abs() > 2.0 {
                    // Height change indicator
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: 140.0,
                        y: 50.0,
                        radius: 8.0,
                        color: if base_lift > 0.0 { Color::LightGreen } else { Color::LightRed },
                    });
                    
                    // Draw height as visual bar
                    let height_bar = (base_lift.abs() * 2.0).min(25.0);
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: 140.0,
                        y1: 50.0,
                        x2: 140.0,
                        y2: if base_lift > 0.0 { 50.0 + height_bar } else { 50.0 - height_bar },
                        color: if base_lift > 0.0 { Color::LightGreen } else { Color::LightRed },
                    });
                }
                
                // Draw real-time angle readouts as position indicators
                if tilt_magnitude > 0.5 {
                    let angle_indicator_radius = platform_radius * 1.3;
                    
                    // Roll angle indicator
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: roll_angle * 3.0,
                        y: angle_indicator_radius,
                        radius: 4.0,
                        color: Color::Magenta,
                    });
                    
                    // Pitch angle indicator  
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: angle_indicator_radius,
                        y: pitch_angle * 3.0,
                        radius: 4.0,
                        color: Color::Cyan,
                    });
                }
            })
            .x_bounds([-200.0, 200.0])  // Maximum resolution bounds
            .y_bounds([-120.0, 120.0]);
        frame.render_widget(gimbal_canvas, main_chunks[1]);
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
