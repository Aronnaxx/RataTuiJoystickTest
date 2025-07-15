# 🎮 EPL Parallel Plate Gimbal Visualizer

A real-time visualization tool for gamepad/joystick input with accurate EPL parallel plate gimbal simulation. Built with Rust using the `gilrs` library for gamepad input and `ratatui` for terminal-based UI.

## Features

- **Real-time Gamepad Detection**: Automatically detects and displays connected gamepads
- **EPL Gimbal Simulation**: Accurate representation of EPL Model GIM-3X-3-F parallel plate gimbal mechanics
- **Multiple Device Support**: Can display multiple gamepads simultaneously
- **Activity Filtering**: Shows only active controllers (with recent input) by default
- **3D SpaceMouse Support**: Full support for 3D input devices like SpaceMouse

## Control Bindings

### Gimbal Control (EPL Parallel Plate System)

| Input Axis | Gimbal Function | Description |
|------------|----------------|-------------|
| **Left Stick X** | **Roll** | Tilts the upper plate left/right (±20°) |
| **Left Stick Y** | **Pitch** | Tilts the upper plate forward/back (±20°) |
| **Z-Axis** (Trigger/SpaceMouse) | **Height** | Raises/lowers entire platform (±15mm) |

### SpaceMouse Axes (if available)
- **Tx** - Translation X (lateral movement)
- **Ty** - Translation Y (forward/back movement)  
- **Tz** - Translation Z (up/down movement) → **Gimbal Height**
- **Rx** - Rotation X → **Gimbal Pitch**
- **Ry** - Rotation Y → **Gimbal Roll**
- **Rz** - Rotation Z (yaw rotation)

### Application Controls

| Key | Function |
|-----|----------|
| `q` or `Esc` | Quit application |
| `d` or `D` | Toggle debug mode (show all devices vs. active only) |

## Gimbal Mechanics

The visualization accurately represents the EPL parallel plate gimbal system:

### Hardware Components
- **6 Scissor Lifts**: Positioned at 60° intervals around the platform
- **Stepper Motors**: Blue circles at the base of each lift
- **Upper Plate**: Tilts based on individual scissor extensions
- **Parallel Linkage**: Maintains plate orientation while allowing tilt

### Visual Indicators
- **🟢 Green Lifts**: Extended (above neutral)
- **🔴 Red Lifts**: Retracted (below neutral)  
- **🟡 Yellow Lifts**: Neutral position
- **Cyan Lines**: Pitch and roll tilt indicators on upper plate
- **Status Dots**: Red dot appears during significant tilt, green/red for height changes

## Installation & Usage

### Prerequisites
- Rust (latest stable version)
- Connected gamepad or 3D input device

### Quick Start
```bash
# Clone and run
git clone <repository-url>
cd joystick_test
cargo run
```

### Building
```bash
cargo build --release
```

## Technical Details

### Dependencies
- `gilrs 0.11.0` - Cross-platform gamepad input
- `ratatui 0.29.0` - Terminal user interface framework
- `crossterm 0.29.0` - Terminal control and keyboard input

### Performance
- **60 FPS** real-time updates
- **16ms** frame time for smooth visualization
- Automatic device activity tracking with 30-second timeout

### Supported Devices
- Standard USB/Bluetooth gamepads (Xbox, PlayStation, etc.)
- 3D SpaceMouse devices
- Flight simulation controllers
- Any HID-compliant input device with analog axes

## Gimbal Physics

The simulation uses realistic physics calculations:

```rust
// Each scissor lift height calculation
let pitch_effect = (base_y / platform_radius) * pitch_angle.to_radians() * platform_radius;
let roll_effect = (base_x / platform_radius) * roll_angle.to_radians() * platform_radius;
let scissor_height = nominal_height + pitch_effect + roll_effect + base_lift;
```

This ensures the upper plate achieves the correct tilt angle through coordinated scissor lift extension/retraction, just like the real EPL hardware.

## Troubleshooting

### No Gamepads Detected
- Ensure your device is properly connected
- Try pressing `d` to show all devices (including inactive ones)
- Some devices may require driver installation

### Low Sensitivity
- Check your device's calibration in system settings
- Some SpaceMouse devices have adjustable sensitivity settings

### Performance Issues
- Close other applications using the gamepad
- Ensure terminal window is properly sized
- Try reducing terminal font size for better graphics resolution

## License

This project is open source. See LICENSE file for details.

---

**Compatible with EPL Model GIM-3X-3-F and similar parallel plate gimbal systems.**
