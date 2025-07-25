mod config;
mod gimbal;

use config::Config;
use gimbal::{GimbalController, InputState};
use gilrs::{Gilrs, Event, Axis, Button};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, List, ListItem},
    widgets::canvas::Canvas,
    Frame, Terminal,
};
use crossterm::{
    event::{self, Event as CrosstermEvent, KeyCode, KeyEventKind},
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
    config: Config,
    gimbal_controller: GimbalController,
    input_state: InputState,
    gilrs: Gilrs,
    gamepads: HashMap<gilrs::GamepadId, GamepadState>,
    running: bool,
    debug_mode: bool,
}

impl App {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let config = Config::load_or_create("config.toml")?;
        let gimbal_controller = GimbalController::new(config.clone());
        let gilrs = Gilrs::new().map_err(|e| format!("Failed to initialize gilrs: {}", e))?;
        
        Ok(App {
            debug_mode: config.debug.enabled,
            config,
            gimbal_controller,
            input_state: InputState::default(),
            gilrs,
            gamepads: HashMap::new(),
            running: true,
        })
    }

    fn update(&mut self) {
        // Process gamepad events
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
                    self.input_state.buttons.insert(button, true);
                },
                gilrs::EventType::ButtonReleased(button, _) => {
                    gamepad_state.buttons.insert(button, false);
                    self.input_state.buttons.insert(button, false);
                },
                gilrs::EventType::AxisChanged(axis, value, _) => {
                    gamepad_state.axes.insert(axis, value);
                    self.input_state.axes.insert(axis, value);
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

        // Update gimbal with current input
        self.gimbal_controller.update(&self.input_state);
    }

    fn handle_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.running = false;
            }
            KeyCode::Char('t') => {
                self.debug_mode = !self.debug_mode;
            }
            KeyCode::Char('r') => {
                self.gimbal_controller.reset();
                self.input_state.keyboard_pitch = 0.0;
                self.input_state.keyboard_roll = 0.0;
                self.input_state.keyboard_lift = 0.0;
            }
            KeyCode::Char(c) => {
                self.gimbal_controller.handle_keyboard(&mut self.input_state, c, true);
            }
            _ => {}
        }
    }

    fn draw(&self, frame: &mut Frame) {
        if self.debug_mode {
            self.draw_debug_view(frame);
        } else {
            self.draw_gimbal_view(frame);
        }
    }

    fn draw_debug_view(&self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),     // Header
                Constraint::Min(10),       // Debug info
                Constraint::Min(15),       // Gimbal (smaller)
            ])
            .split(frame.area());

        // Header
        let header = Paragraph::new("🔧 DEBUG MODE - Press 't' to toggle, 'q' to quit, 'r' to reset")
            .block(Block::default().borders(Borders::ALL))
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(header, chunks[0]);

        // Debug info split
        let debug_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),  // Axes
                Constraint::Percentage(50),  // Config & State
            ])
            .split(chunks[1]);

        self.draw_debug_axes(frame, debug_chunks[0]);
        self.draw_debug_state(frame, debug_chunks[1]);
        
        // Smaller gimbal view
        self.draw_gimbal_visualization(frame, chunks[2]);
    }

    fn draw_debug_axes(&self, frame: &mut Frame, area: Rect) {
        let mut items = vec![
            ListItem::new(Line::from(Span::styled("=== ACTIVE AXES ===", Style::default().fg(Color::Cyan)))),
        ];

        // Show all axes with values
        let mut axes_vec: Vec<_> = self.input_state.axes.iter().collect();
        axes_vec.sort_by_key(|(axis, _)| format!("{:?}", axis));

        for (axis, &value) in axes_vec {
            let color = if value.abs() > 0.1 {
                Color::Green
            } else if value.abs() > 0.01 {
                Color::Yellow
            } else {
                Color::Gray
            };

            items.push(ListItem::new(Line::from(Span::styled(
                format!("{:?}: {:.3}", axis, value),
                Style::default().fg(color),
            ))));
        }

        if self.config.debug.show_button_states && !self.input_state.buttons.is_empty() {
            items.push(ListItem::new(Line::from(Span::styled("=== BUTTONS ===", Style::default().fg(Color::Cyan)))));
            for (button, &pressed) in &self.input_state.buttons {
                if pressed {
                    items.push(ListItem::new(Line::from(Span::styled(
                        format!("{:?}: PRESSED", button),
                        Style::default().fg(Color::Red),
                    ))));
                }
            }
        }

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Input Debug"));
        frame.render_widget(list, area);
    }

    fn draw_debug_state(&self, frame: &mut Frame, area: Rect) {
        let state = self.gimbal_controller.get_state();
        let config = self.gimbal_controller.get_config();

        let items = vec![
            ListItem::new(Line::from(Span::styled("=== GIMBAL STATE ===", Style::default().fg(Color::Cyan)))),
            ListItem::new(Line::from(format!("Pitch: {:.1}° (max: ±{:.1}°)", state.pitch, config.gimbal.max_pitch))),
            ListItem::new(Line::from(format!("Roll:  {:.1}° (max: ±{:.1}°)", state.roll, config.gimbal.max_roll))),
            ListItem::new(Line::from(format!("Lift:  {:.1}mm (max: ±{:.1}mm)", state.lift, config.gimbal.max_lift))),
            ListItem::new(Line::from("")),
            ListItem::new(Line::from(Span::styled("=== CONFIG ===", Style::default().fg(Color::Cyan)))),
            ListItem::new(Line::from(format!("Pitch Axis: {}", config.controls.joystick.pitch_axis))),
            ListItem::new(Line::from(format!("Roll Axis:  {}", config.controls.joystick.roll_axis))),
            ListItem::new(Line::from(format!("Lift Axis:  {}", config.controls.joystick.lift_axis))),
            ListItem::new(Line::from("")),
            ListItem::new(Line::from(Span::styled("=== KEYBOARD ===", Style::default().fg(Color::Cyan)))),
            ListItem::new(Line::from(format!("WASD: Pitch/Roll, RF: Lift"))),
            ListItem::new(Line::from(format!("Step: {:.3}", config.controls.keyboard_step))),
        ];

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("State & Config"));
        frame.render_widget(list, area);
    }

    fn draw_gimbal_view(&self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(frame.area());

        // Header
        let state = self.gimbal_controller.get_state();
        let header_text = format!(
            "🎮 EPL Gimbal Controller - Pitch: {:.1}° Roll: {:.1}° Lift: {:.1}mm | 't' debug, 'r' reset, 'q' quit",
            state.pitch, state.roll, state.lift
        );
        let header = Paragraph::new(header_text)
            .block(Block::default().borders(Borders::ALL))
            .style(Style::default().fg(Color::Cyan));
        frame.render_widget(header, chunks[0]);

        self.draw_gimbal_visualization(frame, chunks[1]);
    }

    fn draw_gimbal_visualization(&self, frame: &mut Frame, area: Rect) {
        let state = self.gimbal_controller.get_state();
        
        let gimbal_canvas = Canvas::default()
            .block(Block::default().borders(Borders::ALL)
                .title("🎯 EPL Parallel Plate Gimbal - Isometric View (3 Scissor Lifts)"))
            .paint(|ctx| {
                // Use the processed gimbal state values instead of raw input
                let pitch_angle = state.pitch;  // Already processed by gimbal controller
                let roll_angle = state.roll;    // Already processed by gimbal controller
                let base_lift = state.lift;     // Already processed by gimbal controller

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

                for (i, (angle_deg, radius)) in scissor_positions.iter().enumerate() {
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
                    
                    // Draw realistic large diamond-shaped scissor mechanism - spans nearly entire base plate
                    let scissor_width = platform_radius * 1.2;  // Much larger - nearly touching other lifts
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
                    
                    // Draw the diamond-shaped scissor mechanism (4 main struts forming diamond) - much thicker
                    for thickness in [-3.0, -2.5, -2.0, -1.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0] {
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
                    
                    // Draw horizontal worm gear shaft running through center of diamond (perpendicular to lift) - thicker
                    let worm_start_x = base_x_3d - diamond_offset_x * 0.8;
                    let worm_start_z = base_y_3d - diamond_offset_z * 0.8;
                    let worm_end_x = base_x_3d + diamond_offset_x * 0.8;
                    let worm_end_z = base_y_3d + diamond_offset_z * 0.8;
                    
                    let (worm_start_iso_x, worm_start_iso_y) = to_isometric(worm_start_x, mid_height_3d, worm_start_z);
                    let (worm_end_iso_x, worm_end_iso_y) = to_isometric(worm_end_x, mid_height_3d, worm_end_z);
                    
                    for thickness in [-2.5, -2.0, -1.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5, 2.0, 2.5] {
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
                    
                    // Draw diamond pivot points where struts meet (ball bearings) - larger
                    for (px, py, color, radius) in [
                        (mid_left_x, mid_left_y, Color::White, 4.5),
                        (mid_right_x, mid_right_y, Color::White, 4.5),
                    ] {
                        ctx.draw(&ratatui::widgets::canvas::Circle {
                            x: px,
                            y: py,
                            radius,
                            color,
                        });
                    }
                    
                    // Draw square stepper motor mounted on the moving scissor assembly (moves with lift)
                    let motor_3d_x = base_x_3d + diamond_offset_x * 1.2;
                    let motor_3d_z = base_y_3d + diamond_offset_z * 1.2;
                    let (motor_x, motor_y) = to_isometric(motor_3d_x, mid_height_3d, motor_3d_z);
                    
                    // Draw square motor housing (stepper motors are square, not circular)
                    let motor_size = 8.0;  // Half-size for square motor
                    let motor_corners = [
                        (-motor_size, -motor_size),
                        (motor_size, -motor_size),
                        (motor_size, motor_size),
                        (-motor_size, motor_size),
                    ];
                    
                    // Draw square motor body
                    for i in 0..4 {
                        let (x1, y1) = motor_corners[i];
                        let (x2, y2) = motor_corners[(i + 1) % 4];
                        
                        for thickness in [-2.0, -1.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5, 2.0] {
                            ctx.draw(&ratatui::widgets::canvas::Line {
                                x1: motor_x + x1 + thickness,
                                y1: motor_y + y1,
                                x2: motor_x + x2 + thickness,
                                y2: motor_y + y2,
                                color: Color::Blue,
                            });
                        }
                    }
                    
                    // Draw square motor housing outline
                    let housing_size = motor_size + 2.0;
                    let housing_corners = [
                        (-housing_size, -housing_size),
                        (housing_size, -housing_size),
                        (housing_size, housing_size),
                        (-housing_size, housing_size),
                    ];
                    
                    for i in 0..4 {
                        let (x1, y1) = housing_corners[i];
                        let (x2, y2) = housing_corners[(i + 1) % 4];
                        
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1: motor_x + x1,
                            y1: motor_y + y1,
                            x2: motor_x + x2,
                            y2: motor_y + y2,
                            color: Color::DarkGray,
                        });
                    }
                    
                    // Draw motor connection to worm gear (horizontal drive shaft) - thicker
                    for thickness in [-2.0, -1.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5, 2.0] {
                        ctx.draw(&ratatui::widgets::canvas::Line {
                            x1: motor_x + thickness,
                            y1: motor_y,
                            x2: (worm_start_iso_x + worm_end_iso_x) / 2.0 + thickness,
                            y2: (worm_start_iso_y + worm_end_iso_y) / 2.0,
                            color: Color::DarkGray,
                        });
                    }
                    
                    // Draw mounting brackets for motor (attached to scissor assembly) - thicker
                    let bracket_size = 6.0;  // Larger brackets for bigger motor
                    for bracket_offset in [-bracket_size, bracket_size] {
                        let bracket_3d_x = motor_3d_x + bracket_offset * perpendicular_angle.cos();
                        let bracket_3d_z = motor_3d_z + bracket_offset * perpendicular_angle.sin();
                        let (bracket_x, bracket_y) = to_isometric(bracket_3d_x, mid_height_3d, bracket_3d_z);
                        
                        for thickness in [-1.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5] {
                            ctx.draw(&ratatui::widgets::canvas::Line {
                                x1: motor_x + thickness,
                                y1: motor_y,
                                x2: bracket_x + thickness,
                                y2: bracket_y,
                                color: Color::DarkGray,
                            });
                        }
                    }
                    
                    // Draw connection points - single attachment points like real hardware (larger)
                    // Bottom tip connection (fixed to base)
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: bottom_tip_x,
                        y: bottom_tip_y,
                        radius: 4.5,
                        color: Color::Gray,
                    });
                    
                    // Top tip connection (ball bearing to upper plate)
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: top_tip_x,
                        y: top_tip_y,
                        radius: 5.5,
                        color: Color::LightBlue,
                    });
                    
                    // Draw enhanced ball bearing detail at the top connection - larger
                    // Main ball bearing housing
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: top_tip_x,
                        y: top_tip_y,
                        radius: 7.0,
                        color: Color::White,
                    });
                    // Inner bearing race
                    ctx.draw(&ratatui::widgets::canvas::Circle {
                        x: top_tip_x,
                        y: top_tip_y,
                        radius: 3.5,
                        color: Color::Gray,
                    });
                    
                    // Label the actuators
                    let _label = match i {
                        0 => "A1",
                        1 => "A2", 
                        2 => "A3",
                        _ => "",
                    };
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
        frame.render_widget(gimbal_canvas, area);
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
    println!("Config loaded. Debug mode: {}", app.debug_mode);

    // Main loop
    let tick_rate = Duration::from_millis(16); // ~60 FPS
    let mut last_tick = Instant::now();

    while app.running {
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let CrosstermEvent::Key(key) = event::read()? {
                match key.kind {
                    KeyEventKind::Press => {
                        app.handle_key(key.code);
                    }
                    KeyEventKind::Release => {
                        // Handle key release for WASD movement
                        if let KeyCode::Char(c) = key.code {
                            app.gimbal_controller.handle_keyboard(&mut app.input_state, c, false);
                        }
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