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
            
            // Create a dormant gamepad state for visualization
            let dormant_gamepad = GamepadState {
                name: "No Active Controller - Demo Mode".to_string(),
                connected: false,
                axes: HashMap::new(),
                buttons: HashMap::new(),
                last_activity: None,
            };
            
            // Show status message in header area
            let no_gamepad = Paragraph::new(vec![
                Line::from(Span::styled("🕹️  Demo Mode - No active controllers", Style::default().fg(Color::Yellow))),
                if total_connected > 0 && !self.show_all_devices {
                    Line::from(format!("({} HID device(s) connected but inactive - press 'd' to show all)", total_connected))
                } else if total_connected > 0 {
                    Line::from(format!("({} device(s) connected)", total_connected))
                } else {
                    Line::from("Connect a gamepad or SpaceMouse to control the gimbal")
                },
            ])
                .block(Block::default().borders(Borders::ALL).title("🎯 Status"))
                .style(Style::default());
            
            // Use a smaller area for the status message
            let demo_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(5),    // Status message
                    Constraint::Min(0),       // Gimbal visualization
                ])
                .split(chunks[1]);
                
            frame.render_widget(no_gamepad, demo_chunks[0]);
            
            // Draw the gimbal in demo/dormant state
            self.draw_gamepad(frame, demo_chunks[1], &dormant_gamepad);
            return;
        }

        // Split the main area for multiple displayed gamepads
        let gamepad_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Min(0); displayed_gamepads.len()])
            .split(chunks[1]);

        for (i, (_id, gamepad)) in displayed_gamepads.iter().enumerate() {
            if i >= gamepad_chunks.len() {
                break;
            }

            self.draw_gamepad(frame, gamepad_chunks[i], gamepad);
        }
    }

    fn draw_gamepad(&self, frame: &mut Frame, area: Rect, gamepad: &GamepadState) {
        let (status_color, status_text, title_suffix) = if gamepad.connected { 
            (Color::Green, "Connected", "")
        } else { 
            (Color::Yellow, "Demo Mode", " - Neutral Position")
        };

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
        } else if gamepad.connected {
            "⚫ INACTIVE"
        } else {
            "⚪ DEMO"
        };
        
        let title = format!("🎮 {} ({}) {}{} 🎮", gamepad.name, status_text, activity_indicator, title_suffix);
        let title_widget = Paragraph::new(title)
            .style(Style::default().fg(status_color))
            .block(Block::default().borders(Borders::NONE));
        frame.render_widget(title_widget, main_chunks[0]);

        // Full-screen EPL Gimbal Visualization (Maximum Resolution)
        let gimbal_canvas = Canvas::default()
            .block(Block::default().borders(Borders::ALL).title("🎯 EPL Parallel Plate Gimbal - Isometric View (3 Scissor Lifts)"))
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

                // Isometric projection helper function
                let to_isometric = |x: f64, y: f64, z: f64| -> (f64, f64) {
                    // Standard isometric projection matrix
                    let iso_x = (x - z) * 0.866;  // cos(30°) ≈ 0.866
                    let iso_y = (x + z) * 0.5 + y;  // sin(30°) = 0.5
                    (iso_x, iso_y)
                };

                // Draw base platform (fixed lower plate) in isometric view - triangular support structure
                let base_points = 24;  // Much more segments for ultra-smooth circle
                for i in 0..base_points {
                    let angle1 = i as f64 * 2.0 * std::f64::consts::PI / base_points as f64;
                    let angle2 = (i + 1) as f64 * 2.0 * std::f64::consts::PI / base_points as f64;
                    
                    let x1_3d = platform_radius * angle1.cos();
                    let y1_3d = platform_radius * angle1.sin();
                    let x2_3d = platform_radius * angle2.cos();
                    let y2_3d = platform_radius * angle2.sin();
                    
                    let (x1, y1) = to_isometric(x1_3d, base_height, y1_3d);
                    let (x2, y2) = to_isometric(x2_3d, base_height, y2_3d);
                    
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1, y1, x2, y2,
                        color: Color::DarkGray,
                    });
                }

                // Draw triangular support structure connecting the three scissor lift positions
                for i in 0..3 {
                    let angle1 = (i * 120) as f64;
                    let angle2 = ((i + 1) * 120) as f64;
                    
                    let x1_3d = platform_radius * 0.8 * angle1.to_radians().cos();
                    let y1_3d = platform_radius * 0.8 * angle1.to_radians().sin();
                    let x2_3d = platform_radius * 0.8 * angle2.to_radians().cos();
                    let y2_3d = platform_radius * 0.8 * angle2.to_radians().sin();
                    
                    let (x1, y1) = to_isometric(x1_3d, base_height, y1_3d);
                    let (x2, y2) = to_isometric(x2_3d, base_height, y2_3d);
                    
                    // Draw thick triangular base structure
                    for thickness in [-2.0, -1.0, 0.0, 1.0, 2.0] {
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1: x1 + thickness,
                            y1,
                            x2: x2 + thickness,
                            y2,
                            color: Color::Gray,
                        });
                    }
                }

                // Draw center hub in isometric
                let center_points = 12;
                let hub_radius = platform_radius * 0.3;
                for i in 0..center_points {
                    let angle1 = i as f64 * 2.0 * std::f64::consts::PI / center_points as f64;
                    let angle2 = (i + 1) as f64 * 2.0 * std::f64::consts::PI / center_points as f64;
                    
                    let x1_3d = hub_radius * angle1.cos();
                    let y1_3d = hub_radius * angle1.sin();
                    let x2_3d = hub_radius * angle2.cos();
                    let y2_3d = hub_radius * angle2.sin();
                    
                    let (x1, y1) = to_isometric(x1_3d, base_height, y1_3d);
                    let (x2, y2) = to_isometric(x2_3d, base_height, y2_3d);
                    
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1, y1, x2, y2,
                        color: Color::Gray,
                    });
                }

                // EPL Gimbal: Three scissor lifts in triangular configuration (120° apart)
                let scissor_positions: [(f64, f64); 3] = [
                    (0.0, platform_radius * 0.8),     // Front (0°)
                    (120.0, platform_radius * 0.8),   // Back-right (120°)
                    (240.0, platform_radius * 0.8),   // Back-left (240°)
                ];

                let mut upper_plate_points = Vec::new();

                // Isometric projection helper function
                let to_isometric = |x: f64, y: f64, z: f64| -> (f64, f64) {
                    // Standard isometric projection matrix
                    let iso_x = (x - z) * 0.866;  // cos(30°) ≈ 0.866
                    let iso_y = (x + z) * 0.5 + y;  // sin(30°) = 0.5
                    (iso_x, iso_y)
                };

                for (angle_deg, radius) in scissor_positions.iter() {
                    let angle_rad = angle_deg.to_radians();
                    
                    // 3D position on base (no rotation)
                    let base_x_3d = radius * angle_rad.cos();
                    let base_y_3d = radius * angle_rad.sin();
                    let base_z_3d = base_height;
                    
                    // Convert to isometric view
                    let _base_iso = to_isometric(base_x_3d, base_z_3d, base_y_3d);
                    
                    // Calculate scissor extension based on desired tilt
                    // Each scissor extends/retracts to achieve the plate angle
                    let pitch_effect = (base_y_3d / platform_radius) * pitch_angle.to_radians() * platform_radius;
                    let roll_effect = (base_x_3d / platform_radius) * roll_angle.to_radians() * platform_radius;
                    
                    // Final height for this scissor lift
                    let scissor_height_3d = nominal_height + pitch_effect + roll_effect;
                    
                    // Upper plate connection point (same X,Y as base for parallel linkage)
                    let (upper_x, upper_y) = to_isometric(base_x_3d, scissor_height_3d, base_y_3d);
                    upper_plate_points.push((upper_x, upper_y, scissor_height_3d));
                    
                    // Draw enhanced X-pattern scissor lift mechanism
                    let extension = scissor_height_3d - nominal_height;
                    let lift_color = if extension > 6.0 {
                        Color::LightGreen  // Extended
                    } else if extension < -6.0 {
                        Color::LightRed    // Retracted
                    } else {
                        Color::Yellow      // Neutral
                    };
                    
                    // Draw X-pattern scissor mechanism (EPL style)
                    let scissor_width = 20.0;  // Width of the X pattern
                    let mid_height_3d = (base_height + scissor_height_3d) / 2.0;
                    let (mid_x, mid_y) = to_isometric(base_x_3d, mid_height_3d, base_y_3d);
                    
                    // Calculate X-pattern endpoints for isometric view
                    let x_offset = scissor_width * 0.5;
                    let (left_base_x, left_base_y) = to_isometric(base_x_3d - x_offset, base_height, base_y_3d);
                    let (right_base_x, right_base_y) = to_isometric(base_x_3d + x_offset, base_height, base_y_3d);
                    let (left_top_x, left_top_y) = to_isometric(base_x_3d - x_offset, scissor_height_3d, base_y_3d);
                    let (right_top_x, right_top_y) = to_isometric(base_x_3d + x_offset, scissor_height_3d, base_y_3d);
                    
                    // Draw the X-pattern scissor lifts (thick lines for visibility)
                    for thickness in [-1.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5] {
                        // Left diagonal of X
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1: left_base_x + thickness,
                            y1: left_base_y,
                            x2: right_top_x + thickness,
                            y2: right_top_y,
                            color: lift_color,
                        });
                        
                        // Right diagonal of X
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1: right_base_x + thickness,
                            y1: right_base_y,
                            x2: left_top_x + thickness,
                            y2: left_top_y,
                            color: lift_color,
                        });
                    }
                    
                    // Draw center pivot point of X
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: mid_x,
                        y: mid_y,
                        radius: 4.0,
                        color: Color::White,
                    });
                    
                    // Draw stepper motor housing at base (isometric view)
                    let (motor_x, motor_y) = to_isometric(base_x_3d, base_height - 10.0, base_y_3d);
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: motor_x,
                        y: motor_y,
                        radius: 8.0,
                        color: Color::Blue,
                    });
                    
                    // Draw motor mounting bracket in isometric
                    let (bracket_left_x, bracket_left_y) = to_isometric(base_x_3d - 6.0, base_height - 10.0, base_y_3d);
                    let (bracket_right_x, bracket_right_y) = to_isometric(base_x_3d + 6.0, base_height - 10.0, base_y_3d);
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: bracket_left_x,
                        y1: bracket_left_y,
                        x2: bracket_right_x,
                        y2: bracket_right_y,
                        color: Color::DarkGray,
                    });
                    
                    // Draw base connection points
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: left_base_x,
                        y: left_base_y,
                        radius: 3.0,
                        color: Color::Gray,
                    });
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: right_base_x,
                        y: right_base_y,
                        radius: 3.0,
                        color: Color::Gray,
                    });
                    
                    // Draw upper connection points
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: left_top_x,
                        y: left_top_y,
                        radius: 3.0,
                        color: Color::LightBlue,
                    });
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: right_top_x,
                        y: right_top_y,
                        radius: 3.0,
                        color: Color::LightBlue,
                    });
                }

                // Draw upper platform (tilted based on scissor heights) in isometric view
                // Draw triangular upper plate connecting the three scissor tops
                for i in 0..upper_plate_points.len() {
                    let (x1, y1, h1) = upper_plate_points[i];
                    let (x2, y2, h2) = upper_plate_points[(i + 1) % upper_plate_points.len()];
                    
                    // Draw upper plate edge using isometric coordinates
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
                    
                    // Draw thick triangular upper plate edges
                    for thickness in [-1.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5] {
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1: x1 + thickness, y1, x2: x2 + thickness, y2,
                            color: line_color,
                        });
                    }
                    
                    // Draw connection points where upper plate meets scissor lifts
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: x1,
                        y: y1,
                        radius: 4.0,
                        color: Color::LightBlue,
                    });
                }
                
                // Draw upper plate center area (triangular fill pattern)
                let center_height = nominal_height;
                let (center_x, center_y) = to_isometric(0.0, center_height, 0.0);
                
                // Draw radial lines from center to each vertex for triangular pattern
                for (x, y, _h) in &upper_plate_points {
                    for thickness in [-0.5, 0.0, 0.5] {
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1: center_x + thickness,
                            y1: center_y,
                            x2: x + thickness,
                            y2: *y,
                            color: Color::DarkGray,
                        });
                    }
                }

                // Draw center payload mount on upper plate in isometric view
                let center_height = nominal_height + 
                    (pitch_angle.to_radians() * 0.0) +  // Center doesn't move much for small tilts
                    (roll_angle.to_radians() * 0.0);
                    
                let _payload_center = to_isometric(0.0, center_height + 5.0, 0.0);
                
                // Main payload mounting ring
                let ring_points = 16;
                let mount_radius = 15.0;
                for i in 0..ring_points {
                    let angle1 = i as f64 * 2.0 * std::f64::consts::PI / ring_points as f64;
                    let angle2 = (i + 1) as f64 * 2.0 * std::f64::consts::PI / ring_points as f64;
                    
                    let x1_3d = mount_radius * angle1.cos();
                    let y1_3d = mount_radius * angle1.sin();
                    let x2_3d = mount_radius * angle2.cos();
                    let y2_3d = mount_radius * angle2.sin();
                    
                    let (x1, y1) = to_isometric(x1_3d, center_height + 5.0, y1_3d);
                    let (x2, y2) = to_isometric(x2_3d, center_height + 5.0, y2_3d);
                    
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1, y1, x2, y2,
                        color: Color::LightCyan,
                    });
                }
                
                // Inner mounting ring
                let inner_radius = 10.0;
                for i in 0..ring_points {
                    let angle1 = i as f64 * 2.0 * std::f64::consts::PI / ring_points as f64;
                    let angle2 = (i + 1) as f64 * 2.0 * std::f64::consts::PI / ring_points as f64;
                    
                    let x1_3d = inner_radius * angle1.cos();
                    let y1_3d = inner_radius * angle1.sin();
                    let x2_3d = inner_radius * angle2.cos();
                    let y2_3d = inner_radius * angle2.sin();
                    
                    let (x1, y1) = to_isometric(x1_3d, center_height + 5.0, y1_3d);
                    let (x2, y2) = to_isometric(x2_3d, center_height + 5.0, y2_3d);
                    
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1, y1, x2, y2,
                        color: Color::Cyan,
                    });
                }
                
                // Draw payload mounting bolt holes (positioned at 120° intervals for triangular pattern)
                let bolt_radius = 12.0;
                for i in 0..3 {
                    let angle = i as f64 * 2.0 * std::f64::consts::PI / 3.0; // 120° spacing
                    let x_3d = bolt_radius * angle.cos();
                    let y_3d = bolt_radius * angle.sin();
                    let (bolt_x, bolt_y) = to_isometric(x_3d, center_height + 5.0, y_3d);
                    
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: bolt_x,
                        y: bolt_y,
                        radius: 3.0,
                        color: Color::DarkGray,
                    });
                }

                // Draw enhanced tilt visualization lines in isometric view
                let tilt_line_length = platform_radius * 0.7;
                
                // Roll tilt line (left-right axis) in isometric
                let roll_tilt_height = roll_angle.to_radians() * tilt_line_length * 0.3;
                let (tilt_left_x, tilt_left_y) = to_isometric(-tilt_line_length, center_height - roll_tilt_height, 0.0);
                let (tilt_right_x, tilt_right_y) = to_isometric(tilt_line_length, center_height + roll_tilt_height, 0.0);
                
                for thickness in [-2.0, -1.0, 0.0, 1.0, 2.0] {
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: tilt_left_x + thickness,
                        y1: tilt_left_y,
                        x2: tilt_right_x + thickness,
                        y2: tilt_right_y,
                        color: Color::Magenta,
                    });
                }
                
                // Pitch tilt line (forward-back axis) in isometric
                let pitch_tilt_height = pitch_angle.to_radians() * tilt_line_length * 0.3;
                let (tilt_front_x, tilt_front_y) = to_isometric(0.0, center_height - pitch_tilt_height, -tilt_line_length);
                let (tilt_back_x, tilt_back_y) = to_isometric(0.0, center_height + pitch_tilt_height, tilt_line_length);
                
                for thickness in [-2.0, -1.0, 0.0, 1.0, 2.0] {
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: tilt_front_x + thickness,
                        y1: tilt_front_y,
                        x2: tilt_back_x + thickness,
                        y2: tilt_back_y,
                        color: Color::Cyan,
                    });
                }

                // Draw enhanced coordinate system reference in isometric view
                let coord_origin_3d = (-140.0, -80.0, 0.0);
                let (coord_x, coord_y) = to_isometric(coord_origin_3d.0, coord_origin_3d.1, coord_origin_3d.2);
                
                // X-axis (Roll) - Red
                let (x_end_x, x_end_y) = to_isometric(coord_origin_3d.0 + 30.0, coord_origin_3d.1, coord_origin_3d.2);
                for thickness in [-1.0, 0.0, 1.0] {
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: coord_x + thickness, y1: coord_y, x2: x_end_x + thickness, y2: x_end_y,
                        color: Color::Red,
                    });
                }
                
                // Y-axis (Height) - Green  
                let (y_end_x, y_end_y) = to_isometric(coord_origin_3d.0, coord_origin_3d.1 + 30.0, coord_origin_3d.2);
                for thickness in [-1.0, 0.0, 1.0] {
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: coord_x + thickness, y1: coord_y, x2: y_end_x + thickness, y2: y_end_y,
                        color: Color::Green,
                    });
                }
                
                // Z-axis (Pitch) - Blue
                let (z_end_x, z_end_y) = to_isometric(coord_origin_3d.0, coord_origin_3d.1, coord_origin_3d.2 + 30.0);
                for thickness in [-1.0, 0.0, 1.0] {
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: coord_x + thickness, y1: coord_y, x2: z_end_x + thickness, y2: z_end_y,
                        color: Color::Blue,
                    });
                }

                // Enhanced status indicators in isometric view
                let tilt_magnitude = (pitch_angle.powi(2) + roll_angle.powi(2)).sqrt();
                if tilt_magnitude > 2.0 {
                    // Tilt warning indicator
                    let (warning_x, warning_y) = to_isometric(120.0, 80.0, 20.0);
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: warning_x,
                        y: warning_y,
                        radius: 8.0,
                        color: Color::Red,
                    });
                    
                    // Draw angle magnitude as visual bar
                    let bar_length = (tilt_magnitude * 3.0).min(30.0);
                    let (bar_start_x, bar_start_y) = to_isometric(120.0 - bar_length / 2.0, 65.0, 20.0);
                    let (bar_end_x, bar_end_y) = to_isometric(120.0 + bar_length / 2.0, 65.0, 20.0);
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: bar_start_x,
                        y1: bar_start_y,
                        x2: bar_end_x,
                        y2: bar_end_y,
                        color: Color::Red,
                    });
                }
                
                if base_lift.abs() > 2.0 {
                    // Height change indicator
                    let (height_ind_x, height_ind_y) = to_isometric(120.0, 50.0, 0.0);
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: height_ind_x,
                        y: height_ind_y,
                        radius: 8.0,
                        color: if base_lift > 0.0 { Color::LightGreen } else { Color::LightRed },
                    });
                    
                    // Draw height as visual bar
                    let height_bar = (base_lift.abs() * 2.0).min(25.0);
                    let bar_end_height = if base_lift > 0.0 { 50.0 + height_bar } else { 50.0 - height_bar };
                    let (height_bar_end_x, height_bar_end_y) = to_isometric(120.0, bar_end_height, 0.0);
                    
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: height_ind_x,
                        y1: height_ind_y,
                        x2: height_bar_end_x,
                        y2: height_bar_end_y,
                        color: if base_lift > 0.0 { Color::LightGreen } else { Color::LightRed },
                    });
                }
                
                // Draw real-time angle readouts as position indicators in isometric view
                if tilt_magnitude > 0.5 {
                    let angle_indicator_radius = platform_radius * 1.1;
                    
                    // Roll angle indicator
                    let (roll_ind_x, roll_ind_y) = to_isometric(roll_angle * 3.0, angle_indicator_radius, 0.0);
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: roll_ind_x,
                        y: roll_ind_y,
                        radius: 4.0,
                        color: Color::Magenta,
                    });
                    
                    // Pitch angle indicator  
                    let (pitch_ind_x, pitch_ind_y) = to_isometric(0.0, angle_indicator_radius, pitch_angle * 3.0);
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: pitch_ind_x,
                        y: pitch_ind_y,
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
