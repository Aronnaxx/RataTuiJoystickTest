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
                let pitch_angle = (pitch * 20.0) as f64;  // ±20 degrees max tilt
                let roll_angle = (roll * 20.0) as f64;    // ±20 degrees max tilt
                let base_lift = (z_movement * 15.0) as f64;  // ±15mm vertical movement

                // Platform dimensions - optimized for clear visualization (more squat design)
                let platform_radius = 100.0;  
                let base_height = -30.0;  // Raised base height for more squat appearance
                let nominal_height = 15.0 + base_lift;  // Lower nominal height for closer plates

                // Improved isometric projection helper function
                let to_isometric = |x: f64, y: f64, z: f64| -> (f64, f64) {
                    // Standard isometric projection with proper orientation
                    let iso_x = (x - z) * 0.866;  // cos(30°) ≈ 0.866
                    let iso_y = (x + z) * 0.5 + y;  // sin(30°) = 0.5
                    (iso_x, iso_y)
                };

                // Draw base platform (lower circular plate) - more prominent like real gimbal
                let base_points = 32;  // High resolution circle
                for i in 0..base_points {
                    let angle1 = i as f64 * 2.0 * std::f64::consts::PI / base_points as f64;
                    let angle2 = (i + 1) as f64 * 2.0 * std::f64::consts::PI / base_points as f64;
                    
                    let x1_3d = platform_radius * angle1.cos();
                    let y1_3d = platform_radius * angle1.sin();
                    let x2_3d = platform_radius * angle2.cos();
                    let y2_3d = platform_radius * angle2.sin();
                    
                    let (x1, y1) = to_isometric(x1_3d, base_height, y1_3d);
                    let (x2, y2) = to_isometric(x2_3d, base_height, y2_3d);
                    
                    // Draw thick circular base platform edge
                    for thickness in [-2.0, -1.0, 0.0, 1.0, 2.0] {
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1: x1 + thickness, y1, x2: x2 + thickness, y2,
                            color: Color::Gray,
                        });
                    }
                }

                // Draw inner circular rings on base platform for depth
                for ring_factor in [0.7, 0.5, 0.3] {
                    let ring_radius = platform_radius * ring_factor;
                    for i in 0..24 {
                        let angle1 = i as f64 * 2.0 * std::f64::consts::PI / 24.0;
                        let angle2 = (i + 1) as f64 * 2.0 * std::f64::consts::PI / 24.0;
                        
                        let x1_3d = ring_radius * angle1.cos();
                        let y1_3d = ring_radius * angle1.sin();
                        let x2_3d = ring_radius * angle2.cos();
                        let y2_3d = ring_radius * angle2.sin();
                        
                        let (x1, y1) = to_isometric(x1_3d, base_height, y1_3d);
                        let (x2, y2) = to_isometric(x2_3d, base_height, y2_3d);
                        
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1, y1, x2, y2,
                            color: Color::DarkGray,
                        });
                    }
                }

                // EPL Gimbal: Three scissor lifts at 0°, 120°, 240° (triangular configuration)
                let scissor_positions: [(f64, f64); 3] = [
                    (0.0, platform_radius * 0.75),     // Front (0°)
                    (120.0, platform_radius * 0.75),   // Back-right (120°)
                    (240.0, platform_radius * 0.75),   // Back-left (240°)
                ];

                let mut upper_plate_points = Vec::new();

                for (angle_deg, radius) in scissor_positions.iter() {
                    let angle_rad = angle_deg.to_radians();
                    
                    // 3D position on base platform
                    let base_x_3d = radius * angle_rad.cos();
                    let base_y_3d = radius * angle_rad.sin();
                    
                    // Calculate scissor extension based on desired tilt angles
                    // More realistic gimbal mechanics - each actuator controls plate tilt
                    let pitch_effect = (base_y_3d / platform_radius) * pitch_angle.to_radians() * platform_radius * 0.5;
                    let roll_effect = (base_x_3d / platform_radius) * roll_angle.to_radians() * platform_radius * 0.5;
                    
                    // Final height for this scissor lift
                    let scissor_height_3d = nominal_height + pitch_effect + roll_effect;
                    
                    // Store upper plate connection point
                    let (upper_x, upper_y) = to_isometric(base_x_3d, scissor_height_3d, base_y_3d);
                    upper_plate_points.push((upper_x, upper_y, scissor_height_3d));
                    
                    // Determine scissor lift color based on extension
                    let extension = scissor_height_3d - nominal_height;
                    let lift_color = if extension > 3.0 {
                        Color::LightGreen  // Extended
                    } else if extension < -3.0 {
                        Color::LightRed    // Retracted
                    } else {
                        Color::Yellow      // Neutral
                    };
                    
                    // Draw realistic large diamond-shaped scissor mechanism - spans most of base plate
                    let scissor_width = platform_radius * 0.6;  // Much larger - spans most of base plate like real hardware
                    let mid_height_3d = (base_height + scissor_height_3d) / 2.0;
                    
                    // Calculate diamond pattern endpoints - single points at tips like real hardware
                    let diamond_half_width = scissor_width * 0.5;
                    
                    // Diamond tips - single attachment points (not scaffold)
                    let (bottom_tip_x, bottom_tip_y) = to_isometric(base_x_3d, base_height, base_y_3d);
                    let (top_tip_x, top_tip_y) = to_isometric(base_x_3d, scissor_height_3d, base_y_3d);
                    
                    // Middle diamond points (wider diamond when extended, narrower when compressed)
                    let compression_factor = (scissor_height_3d - nominal_height) / nominal_height;
                    let current_width = diamond_half_width * (1.0 - compression_factor * 0.3);
                    
                    // Calculate proper orientation for diamond scissor lift based on angle
                    let perpendicular_angle = angle_rad + std::f64::consts::PI / 2.0;
                    
                    // Diamond points oriented perpendicular to radius for proper scissors orientation
                    let diamond_offset_x = current_width * perpendicular_angle.cos();
                    let diamond_offset_z = current_width * perpendicular_angle.sin();
                    
                    let (mid_left_x, mid_left_y) = to_isometric(base_x_3d - diamond_offset_x, mid_height_3d, base_y_3d - diamond_offset_z);
                    let (mid_right_x, mid_right_y) = to_isometric(base_x_3d + diamond_offset_x, mid_height_3d, base_y_3d + diamond_offset_z);
                    
                    // Draw the diamond-shaped scissor mechanism (4 main struts forming diamond)
                    for thickness in [-1.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5] {
                        // Four main diamond struts
                        // Bottom tip to left middle
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1: bottom_tip_x + thickness,
                            y1: bottom_tip_y,
                            x2: mid_left_x + thickness,
                            y2: mid_left_y,
                            color: lift_color,
                        });
                        
                        // Bottom tip to right middle  
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1: bottom_tip_x + thickness,
                            y1: bottom_tip_y,
                            x2: mid_right_x + thickness,
                            y2: mid_right_y,
                            color: lift_color,
                        });
                        
                        // Left middle to top tip
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1: mid_left_x + thickness,
                            y1: mid_left_y,
                            x2: top_tip_x + thickness,
                            y2: top_tip_y,
                            color: lift_color,
                        });
                        
                        // Right middle to top tip
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1: mid_right_x + thickness,
                            y1: mid_right_y,
                            x2: top_tip_x + thickness,
                            y2: top_tip_y,
                            color: lift_color,
                        });
                    }
                    
                    // Draw horizontal worm gear shaft running through center of diamond (perpendicular to lift)
                    let worm_start_x = base_x_3d - diamond_offset_x * 0.8;
                    let worm_start_z = base_y_3d - diamond_offset_z * 0.8;
                    let worm_end_x = base_x_3d + diamond_offset_x * 0.8;
                    let worm_end_z = base_y_3d + diamond_offset_z * 0.8;
                    
                    let (worm_start_iso_x, worm_start_iso_y) = to_isometric(worm_start_x, mid_height_3d, worm_start_z);
                    let (worm_end_iso_x, worm_end_iso_y) = to_isometric(worm_end_x, mid_height_3d, worm_end_z);
                    
                    for thickness in [-1.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5] {
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1: worm_start_iso_x + thickness,
                            y1: worm_start_iso_y,
                            x2: worm_end_iso_x + thickness,
                            y2: worm_end_iso_y,
                            color: Color::DarkGray,
                        });
                    }
                    
                    // Draw threaded pattern on worm gear shaft
                    let thread_segments = 8;
                    for i in 0..thread_segments {
                        let t = i as f64 / thread_segments as f64;
                        let thread_x = worm_start_x + (worm_end_x - worm_start_x) * t;
                        let thread_z = worm_start_z + (worm_end_z - worm_start_z) * t;
                        let thread_offset = (i % 2) as f64 * 2.0 - 1.0; // Alternating offset for threads
                        
                        let (thread_iso_x, thread_iso_y) = to_isometric(thread_x, mid_height_3d + thread_offset, thread_z);
                        ctx.draw(&ratatui::widgets::canvas::Circle {
                            x: thread_iso_x,
                            y: thread_iso_y,
                            radius: 1.0,
                            color: Color::Gray,
                        });
                    }
                    
                    // Draw diamond pivot points where struts meet (ball bearings)
                    for (px, py, color, radius) in [
                        (mid_left_x, mid_left_y, Color::White, 3.0),
                        (mid_right_x, mid_right_y, Color::White, 3.0),
                    ] {
                        ctx.draw(&ratatui::widgets::canvas::Circle {
                            x: px,
                            y: py,
                            radius,
                            color,
                        });
                    }
                    
                    // Draw stepper motor mounted on the moving scissor assembly (moves with lift)
                    let motor_3d_x = base_x_3d + diamond_offset_x * 1.2;
                    let motor_3d_z = base_y_3d + diamond_offset_z * 1.2;
                    let (motor_x, motor_y) = to_isometric(motor_3d_x, mid_height_3d, motor_3d_z);
                    
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: motor_x,
                        y: motor_y,
                        radius: 6.0,  // Motor housing
                        color: Color::Blue,
                    });
                    
                    // Draw motor housing and mounting bracket
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: motor_x,
                        y: motor_y,
                        radius: 8.0,
                        color: Color::DarkGray,
                    });
                    
                    // Draw motor connection to worm gear (horizontal drive shaft)
                    for thickness in [-1.0, 0.0, 1.0] {
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1: motor_x + thickness,
                            y1: motor_y,
                            x2: (worm_start_iso_x + worm_end_iso_x) / 2.0 + thickness,
                            y2: (worm_start_iso_y + worm_end_iso_y) / 2.0,
                            color: Color::DarkGray,
                        });
                    }
                    
                    // Draw mounting brackets for motor (attached to scissor assembly)
                    let bracket_size = 4.0;
                    for bracket_offset in [-bracket_size, bracket_size] {
                        let bracket_3d_x = motor_3d_x + bracket_offset * perpendicular_angle.cos();
                        let bracket_3d_z = motor_3d_z + bracket_offset * perpendicular_angle.sin();
                        let (bracket_x, bracket_y) = to_isometric(bracket_3d_x, mid_height_3d, bracket_3d_z);
                        
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1: motor_x,
                            y1: motor_y,
                            x2: bracket_x,
                            y2: bracket_y,
                            color: Color::DarkGray,
                        });
                    }
                    
                    // Draw connection points - single attachment points like real hardware
                    // Bottom tip connection (fixed to base)
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: bottom_tip_x,
                        y: bottom_tip_y,
                        radius: 3.0,
                        color: Color::Gray,
                    });
                    
                    // Top tip connection (ball bearing to upper plate)
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: top_tip_x,
                        y: top_tip_y,
                        radius: 4.0,
                        color: Color::LightBlue,
                    });
                    
                    // Draw enhanced ball bearing detail at the top connection
                    // Main ball bearing housing
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: top_tip_x,
                        y: top_tip_y,
                        radius: 5.0,
                        color: Color::White,
                    });
                    // Inner bearing race
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: top_tip_x,
                        y: top_tip_y,
                        radius: 2.5,
                        color: Color::Gray,
                    });
                }

                // Draw upper platform (circular plate like the real gimbal)
                // First, calculate the average height and tilt of the upper plate
                let avg_height = upper_plate_points.iter().map(|(_, _, h)| h).sum::<f64>() / upper_plate_points.len() as f64;
                
                // Draw the main circular upper plate
                let upper_points = 32;
                for i in 0..upper_points {
                    let angle1 = i as f64 * 2.0 * std::f64::consts::PI / upper_points as f64;
                    let angle2 = (i + 1) as f64 * 2.0 * std::f64::consts::PI / upper_points as f64;
                    
                    // Calculate height variation due to tilt
                    let x1_3d = platform_radius * 0.9 * angle1.cos();
                    let y1_3d = platform_radius * 0.9 * angle1.sin();
                    let x2_3d = platform_radius * 0.9 * angle2.cos();
                    let y2_3d = platform_radius * 0.9 * angle2.sin();
                    
                    // Apply tilt effects to height
                    let pitch_effect1 = (y1_3d / platform_radius) * pitch_angle.to_radians() * platform_radius * 0.5;
                    let roll_effect1 = (x1_3d / platform_radius) * roll_angle.to_radians() * platform_radius * 0.5;
                    let h1 = avg_height + pitch_effect1 + roll_effect1;
                    
                    let pitch_effect2 = (y2_3d / platform_radius) * pitch_angle.to_radians() * platform_radius * 0.5;
                    let roll_effect2 = (x2_3d / platform_radius) * roll_angle.to_radians() * platform_radius * 0.5;
                    let h2 = avg_height + pitch_effect2 + roll_effect2;
                    
                    let (x1, y1) = to_isometric(x1_3d, h1, y1_3d);
                    let (x2, y2) = to_isometric(x2_3d, h2, y2_3d);
                    
                    // Draw the upper plate edge with varying brightness based on height
                    let avg_edge_height = (h1 + h2) / 2.0;
                    let brightness = ((avg_edge_height - (nominal_height - 5.0)) / 15.0).clamp(0.0, 1.0);
                    
                    let line_color = if brightness > 0.8 {
                        Color::White
                    } else if brightness > 0.5 {
                        Color::Gray
                    } else {
                        Color::DarkGray
                    };
                    
                    // Draw thick upper plate edge
                    for thickness in [-1.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5] {
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1: x1 + thickness, y1, x2: x2 + thickness, y2,
                            color: line_color,
                        });
                    }
                }
                
                // Draw connection lines from scissor tops to upper plate edge
                for (upper_x, upper_y, _h) in &upper_plate_points {
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: *upper_x,
                        y: *upper_y,
                        radius: 4.0,
                        color: Color::LightBlue,
                    });
                }
                
                // Draw inner rings on upper plate for structural detail
                for ring_factor in [0.7, 0.5] {
                    let ring_radius = platform_radius * 0.9 * ring_factor;
                    for i in 0..24 {
                        let angle1 = i as f64 * 2.0 * std::f64::consts::PI / 24.0;
                        let angle2 = (i + 1) as f64 * 2.0 * std::f64::consts::PI / 24.0;
                        
                        let x1_3d = ring_radius * angle1.cos();
                        let y1_3d = ring_radius * angle1.sin();
                        let x2_3d = ring_radius * angle2.cos();
                        let y2_3d = ring_radius * angle2.sin();
                        
                        // Apply same tilt effects
                        let pitch_effect1 = (y1_3d / platform_radius) * pitch_angle.to_radians() * platform_radius * 0.5;
                        let roll_effect1 = (x1_3d / platform_radius) * roll_angle.to_radians() * platform_radius * 0.5;
                        let h1 = avg_height + pitch_effect1 + roll_effect1;
                        
                        let pitch_effect2 = (y2_3d / platform_radius) * pitch_angle.to_radians() * platform_radius * 0.5;
                        let roll_effect2 = (x2_3d / platform_radius) * roll_angle.to_radians() * platform_radius * 0.5;
                        let h2 = avg_height + pitch_effect2 + roll_effect2;
                        
                        let (x1, y1) = to_isometric(x1_3d, h1, y1_3d);
                        let (x2, y2) = to_isometric(x2_3d, h2, y2_3d);
                        
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1, y1, x2, y2,
                            color: Color::DarkGray,
                        });
                    }
                }

                // Draw center payload mount on upper plate (adjusted for squat design)
                let center_height = avg_height + 
                    (pitch_angle.to_radians() * 0.0) +  // Center doesn't move much for small tilts
                    (roll_angle.to_radians() * 0.0);
                    
                // Main payload mounting ring
                let ring_points = 16;
                let mount_radius = 10.0;  // Slightly smaller for better proportions
                for i in 0..ring_points {
                    let angle1 = i as f64 * 2.0 * std::f64::consts::PI / ring_points as f64;
                    let angle2 = (i + 1) as f64 * 2.0 * std::f64::consts::PI / ring_points as f64;
                    
                    let x1_3d = mount_radius * angle1.cos();
                    let y1_3d = mount_radius * angle1.sin();
                    let x2_3d = mount_radius * angle2.cos();
                    let y2_3d = mount_radius * angle2.sin();
                    
                    let (x1, y1) = to_isometric(x1_3d, center_height + 2.0, y1_3d);  // Reduced height
                    let (x2, y2) = to_isometric(x2_3d, center_height + 2.0, y2_3d);
                    
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1, y1, x2, y2,
                        color: Color::LightCyan,
                    });
                }
                
                // Inner mounting ring
                let inner_radius = 6.0;  // Proportionally smaller
                for i in 0..ring_points {
                    let angle1 = i as f64 * 2.0 * std::f64::consts::PI / ring_points as f64;
                    let angle2 = (i + 1) as f64 * 2.0 * std::f64::consts::PI / ring_points as f64;
                    
                    let x1_3d = inner_radius * angle1.cos();
                    let y1_3d = inner_radius * angle1.sin();
                    let x2_3d = inner_radius * angle2.cos();
                    let y2_3d = inner_radius * angle2.sin();
                    
                    let (x1, y1) = to_isometric(x1_3d, center_height + 2.0, y1_3d);
                    let (x2, y2) = to_isometric(x2_3d, center_height + 2.0, y2_3d);
                    
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1, y1, x2, y2,
                        color: Color::Cyan,
                    });
                }
                
                // Draw payload mounting bolt holes (3 bolts at 120° spacing)
                let bolt_radius = 8.0;  // Proportionally smaller
                for i in 0..3 {
                    let angle = i as f64 * 2.0 * std::f64::consts::PI / 3.0; // 120° spacing
                    let x_3d = bolt_radius * angle.cos();
                    let y_3d = bolt_radius * angle.sin();
                    let (bolt_x, bolt_y) = to_isometric(x_3d, center_height + 2.0, y_3d);
                    
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: bolt_x,
                        y: bolt_y,
                        radius: 1.5,  // Smaller bolt holes
                        color: Color::DarkGray,
                    });
                }

                // Draw tilt visualization lines
                let tilt_line_length = platform_radius * 0.6;
                
                // Roll tilt line (left-right axis)
                let roll_tilt_height = roll_angle.to_radians() * tilt_line_length * 0.4;
                let (tilt_left_x, tilt_left_y) = to_isometric(-tilt_line_length, center_height - roll_tilt_height, 0.0);
                let (tilt_right_x, tilt_right_y) = to_isometric(tilt_line_length, center_height + roll_tilt_height, 0.0);
                
                for thickness in [-1.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5] {
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: tilt_left_x + thickness,
                        y1: tilt_left_y,
                        x2: tilt_right_x + thickness,
                        y2: tilt_right_y,
                        color: Color::Magenta,
                    });
                }
                
                // Pitch tilt line (forward-back axis)
                let pitch_tilt_height = pitch_angle.to_radians() * tilt_line_length * 0.4;
                let (tilt_front_x, tilt_front_y) = to_isometric(0.0, center_height - pitch_tilt_height, -tilt_line_length);
                let (tilt_back_x, tilt_back_y) = to_isometric(0.0, center_height + pitch_tilt_height, tilt_line_length);
                
                for thickness in [-1.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5] {
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: tilt_front_x + thickness,
                        y1: tilt_front_y,
                        x2: tilt_back_x + thickness,
                        y2: tilt_back_y,
                        color: Color::Cyan,
                    });
                }

                // Draw coordinate system reference
                let coord_origin_3d = (-130.0, -70.0, 0.0);
                let (coord_x, coord_y) = to_isometric(coord_origin_3d.0, coord_origin_3d.1, coord_origin_3d.2);
                
                // X-axis (Roll) - Red
                let (x_end_x, x_end_y) = to_isometric(coord_origin_3d.0 + 25.0, coord_origin_3d.1, coord_origin_3d.2);
                for thickness in [-1.0, 0.0, 1.0] {
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: coord_x + thickness, y1: coord_y, x2: x_end_x + thickness, y2: x_end_y,
                        color: Color::Red,
                    });
                }
                
                // Y-axis (Height) - Green  
                let (y_end_x, y_end_y) = to_isometric(coord_origin_3d.0, coord_origin_3d.1 + 25.0, coord_origin_3d.2);
                for thickness in [-1.0, 0.0, 1.0] {
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: coord_x + thickness, y1: coord_y, x2: y_end_x + thickness, y2: y_end_y,
                        color: Color::Green,
                    });
                }
                
                // Z-axis (Pitch) - Blue
                let (z_end_x, z_end_y) = to_isometric(coord_origin_3d.0, coord_origin_3d.1, coord_origin_3d.2 + 25.0);
                for thickness in [-1.0, 0.0, 1.0] {
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: coord_x + thickness, y1: coord_y, x2: z_end_x + thickness, y2: z_end_y,
                        color: Color::Blue,
                    });
                }

                // Status indicators
                let tilt_magnitude = (pitch_angle.powi(2) + roll_angle.powi(2)).sqrt();
                if tilt_magnitude > 1.0 {
                    // Tilt warning indicator
                    let (warning_x, warning_y) = to_isometric(110.0, 70.0, 15.0);
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: warning_x,
                        y: warning_y,
                        radius: 6.0,
                        color: Color::Red,
                    });
                    
                    // Draw angle magnitude as visual bar
                    let bar_length = (tilt_magnitude * 2.0).min(25.0);
                    let (bar_start_x, bar_start_y) = to_isometric(110.0 - bar_length / 2.0, 60.0, 15.0);
                    let (bar_end_x, bar_end_y) = to_isometric(110.0 + bar_length / 2.0, 60.0, 15.0);
                    for thickness in [-1.0, 0.0, 1.0] {
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1: bar_start_x + thickness,
                            y1: bar_start_y,
                            x2: bar_end_x + thickness,
                            y2: bar_end_y,
                            color: Color::Red,
                        });
                    }
                }
                
                if base_lift.abs() > 1.0 {
                    // Height change indicator
                    let (height_ind_x, height_ind_y) = to_isometric(110.0, 45.0, 0.0);
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: height_ind_x,
                        y: height_ind_y,
                        radius: 6.0,
                        color: if base_lift > 0.0 { Color::LightGreen } else { Color::LightRed },
                    });
                    
                    // Draw height as visual bar
                    let height_bar = (base_lift.abs() * 1.5).min(20.0);
                    let bar_end_height = if base_lift > 0.0 { 45.0 + height_bar } else { 45.0 - height_bar };
                    let (height_bar_end_x, height_bar_end_y) = to_isometric(110.0, bar_end_height, 0.0);
                    
                    for thickness in [-1.0, 0.0, 1.0] {
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1: height_ind_x + thickness,
                            y1: height_ind_y,
                            x2: height_bar_end_x + thickness,
                            y2: height_bar_end_y,
                            color: if base_lift > 0.0 { Color::LightGreen } else { Color::LightRed },
                        });
                    }
                }
                
                // Draw real-time angle readouts as position indicators
                if tilt_magnitude > 0.3 {
                    let angle_indicator_radius = platform_radius * 1.1;
                    
                    // Roll angle indicator
                    let (roll_ind_x, roll_ind_y) = to_isometric(roll_angle * 2.5, angle_indicator_radius, 0.0);
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: roll_ind_x,
                        y: roll_ind_y,
                        radius: 3.0,
                        color: Color::Magenta,
                    });
                    
                    // Pitch angle indicator  
                    let (pitch_ind_x, pitch_ind_y) = to_isometric(0.0, angle_indicator_radius, pitch_angle * 2.5);
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: pitch_ind_x,
                        y: pitch_ind_y,
                        radius: 3.0,
                        color: Color::Cyan,
                    });
                }
            })
            .x_bounds([-180.0, 180.0])  // Optimized bounds for better view
            .y_bounds([-100.0, 100.0]);
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
