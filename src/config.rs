use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub gimbal: GimbalConfig,
    pub controls: ControlsConfig,
    pub debug: DebugConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GimbalConfig {
    pub max_pitch: f64,
    pub max_roll: f64,
    pub max_lift: f64,
    pub pitch_sensitivity: f64,
    pub roll_sensitivity: f64,
    pub lift_sensitivity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlsConfig {
    pub keyboard_enabled: bool,
    pub keyboard_step: f64,
    pub joystick: JoystickConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoystickConfig {
    pub enabled: bool,
    pub pitch_axis: String,
    pub roll_axis: String,
    pub lift_axis: String,
    pub invert_pitch: bool,
    pub invert_roll: bool,
    pub invert_lift: bool,
    pub fallback_axes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugConfig {
    pub enabled: bool,
    pub show_all_axes: bool,
    pub show_button_states: bool,
    pub log_input_values: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            gimbal: GimbalConfig {
                max_pitch: 20.0,
                max_roll: 20.0,
                max_lift: 15.0,
                pitch_sensitivity: 1.0,
                roll_sensitivity: 1.0,
                lift_sensitivity: 1.0,
            },
            controls: ControlsConfig {
                keyboard_enabled: true,
                keyboard_step: 0.1,
                joystick: JoystickConfig {
                    enabled: true,
                    pitch_axis: "RightStickY".to_string(),
                    roll_axis: "RightStickX".to_string(),
                    lift_axis: "RightZ".to_string(),
                    invert_pitch: false,
                    invert_roll: false,
                    invert_lift: false,
                    fallback_axes: vec![
                        "LeftStickY".to_string(),
                        "LeftStickX".to_string(),
                        "LeftZ".to_string(),
                        "Tz".to_string(),
                        "Ty".to_string(),
                        "Tx".to_string(),
                    ],
                },
            },
            debug: DebugConfig {
                enabled: false,
                show_all_axes: true,
                show_button_states: true,
                log_input_values: false,
            },
        }
    }
}

impl Config {
    pub fn load_or_create<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let path = path.as_ref();
        
        if path.exists() {
            let content = fs::read_to_string(path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            let default_config = Config::default();
            let toml_string = toml::to_string_pretty(&default_config)?;
            fs::write(path, toml_string)?;
            println!("Created default config file at {}", path.display());
            Ok(default_config)
        }
    }
}

// Helper to parse axis names to gilrs Axis enum
pub fn parse_axis_name(name: &str) -> Option<gilrs::Axis> {
    match name {
        "LeftStickX" => Some(gilrs::Axis::LeftStickX),
        "LeftStickY" => Some(gilrs::Axis::LeftStickY),
        "LeftZ" => Some(gilrs::Axis::LeftZ),
        "RightStickX" => Some(gilrs::Axis::RightStickX),
        "RightStickY" => Some(gilrs::Axis::RightStickY),
        "RightZ" => Some(gilrs::Axis::RightZ),
        "DPadX" => Some(gilrs::Axis::DPadX),
        "DPadY" => Some(gilrs::Axis::DPadY),
        _ => None,
    }
}