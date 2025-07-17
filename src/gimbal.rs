use crate::config::{Config, parse_axis_name};
use gilrs::{Axis, Button};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct GimbalState {
    pub pitch: f64,  // Forward/back tilt in degrees
    pub roll: f64,   // Left/right tilt in degrees
    pub lift: f64,   // Up/down movement in mm
}

impl Default for GimbalState {
    fn default() -> Self {
        Self {
            pitch: 0.0,
            roll: 0.0,
            lift: 0.0,
        }
    }
}

#[derive(Debug)]
pub struct InputState {
    pub axes: HashMap<Axis, f32>,
    pub buttons: HashMap<Button, bool>,
    pub keyboard_pitch: f64,
    pub keyboard_roll: f64,
    pub keyboard_lift: f64,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            axes: HashMap::new(),
            buttons: HashMap::new(),
            keyboard_pitch: 0.0,
            keyboard_roll: 0.0,
            keyboard_lift: 0.0,
        }
    }
}

pub struct GimbalController {
    config: Config,
    state: GimbalState,
}

impl GimbalController {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            state: GimbalState::default(),
        }
    }

    pub fn update(&mut self, input: &InputState) {
        let mut pitch = 0.0;
        let mut roll = 0.0;
        let mut lift = 0.0;

        // Process joystick input
        if self.config.controls.joystick.enabled {
            pitch += self.get_joystick_axis_value(input, &self.config.controls.joystick.pitch_axis)
                * if self.config.controls.joystick.invert_pitch { -1.0 } else { 1.0 };
            
            roll += self.get_joystick_axis_value(input, &self.config.controls.joystick.roll_axis)
                * if self.config.controls.joystick.invert_roll { -1.0 } else { 1.0 };
            
            lift += self.get_joystick_axis_value(input, &self.config.controls.joystick.lift_axis)
                * if self.config.controls.joystick.invert_lift { -1.0 } else { 1.0 };
        }

        // Process keyboard input
        if self.config.controls.keyboard_enabled {
            pitch += input.keyboard_pitch;
            roll += input.keyboard_roll;
            lift += input.keyboard_lift;
        }

        // Apply sensitivity and limits
        self.state.pitch = (pitch * self.config.gimbal.pitch_sensitivity * self.config.gimbal.max_pitch)
            .clamp(-self.config.gimbal.max_pitch, self.config.gimbal.max_pitch);
        
        self.state.roll = (roll * self.config.gimbal.roll_sensitivity * self.config.gimbal.max_roll)
            .clamp(-self.config.gimbal.max_roll, self.config.gimbal.max_roll);
        
        self.state.lift = (lift * self.config.gimbal.lift_sensitivity * self.config.gimbal.max_lift)
            .clamp(-self.config.gimbal.max_lift, self.config.gimbal.max_lift);

        // Debug logging
        if self.config.debug.log_input_values {
            println!(
                "Input: pitch={:.3}, roll={:.3}, lift={:.3} -> State: pitch={:.1}°, roll={:.1}°, lift={:.1}mm",
                pitch, roll, lift, self.state.pitch, self.state.roll, self.state.lift
            );
        }
    }

    fn get_joystick_axis_value(&self, input: &InputState, axis_name: &str) -> f64 {
        // Try primary axis
        if let Some(axis) = parse_axis_name(axis_name) {
            if let Some(&value) = input.axes.get(&axis) {
                return value as f64;
            }
        }

        // Try fallback axes
        for fallback_name in &self.config.controls.joystick.fallback_axes {
            if let Some(axis) = parse_axis_name(fallback_name) {
                if let Some(&value) = input.axes.get(&axis) {
                    if value.abs() > 0.01 { // Only use if significant input
                        return value as f64;
                    }
                }
            }
        }

        0.0
    }

    pub fn handle_keyboard(&mut self, input: &mut InputState, key: char, pressed: bool) {
        if !self.config.controls.keyboard_enabled {
            return;
        }

        let step = if pressed { self.config.controls.keyboard_step } else { 0.0 };
        
        match key.to_ascii_lowercase() {
            'w' => input.keyboard_pitch = step,      // Pitch forward
            's' => input.keyboard_pitch = -step,     // Pitch back
            'a' => input.keyboard_roll = -step,      // Roll left
            'd' => input.keyboard_roll = step,       // Roll right
            'r' => input.keyboard_lift = step,       // Lift up
            'f' => input.keyboard_lift = -step,      // Lift down
            _ => {}
        }
    }

    pub fn reset(&mut self) {
        self.state = GimbalState::default();
    }

    pub fn get_state(&self) -> &GimbalState {
        &self.state
    }

    pub fn get_config(&self) -> &Config {
        &self.config
    }
}