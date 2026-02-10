# Faderpunk

A powerful, modular eurorack and MIDI synthesizer controller built on the RP2350B microcontroller. Faderpunk provides 16 channels of flexible, programmable control with faders, buttons, CV jacks, and full MIDI integration, all configured through an intuitive web interface.

## Overview

Faderpunk is an embedded Rust project that uses an RP2350B to create a feature-rich eurorack and MIDI controller. Each of the 16 channels can run a different "app" - from LFOs and sequencers to MIDI converters and Turing machines - creating a highly versatile control surface for modular synthesis.

### Key Features

- **16 Independent Channels**: Each channel features a fader, button, RGB LED, and configurable CV jack
- **Modular App Architecture**: Run different apps on different channels simultaneously
- **Dual-Core Performance**: Hardware tasks on Core 0, application logic on Core 1
- **WebUSB Configuration**: Browser-based configurator with drag-and-drop layout management
- **FRAM Storage**: Persistent scene storage with fast save/recall
- **Full MIDI Support**: USB MIDI device capabilities
- **I2C Integration**: Compatible with 16n faderbank protocol
- **Real-time Control**: Async architecture ensures responsive performance

## Hardware Platform

### Microcontroller
- **RP2350B** (Raspberry Pi Pico 2)
- Dual Cortex-M33 cores @ 150 MHz (overclocked to 250MHz)
- 520 KB SRAM

### I/O Components
- **MAX11300**: 20-port programmable mixed-signal I/O (ADC/DAC)
- **FM24V10**: 1 Mbit FRAM for persistent storage
- **WS2812B**: RGB LED chains for visual feedback
- 16 faders, 16 buttons, configurable CV jacks per channel

## Architecture

Faderpunk uses a sophisticated dual-core architecture to maximize performance:

### Core 0: Hardware Management
Runs dedicated Embassy async tasks for hardware interfaces:
- MAX11300 ADC/DAC communication
- Button scanning and debouncing
- LED control (WS2812B)
- MIDI input/output
- FRAM storage operations
- I2C communication
- WebUSB protocol handling

### Core 1: Application Logic
Executes user-facing apps:
- Each app runs as an independent Embassy task
- Apps receive hardware events via PubSub channels
- Apps send commands to hardware via async channels
- Clean abstraction through the `App<N>` API

### Communication
- **Event PubSub**: Broadcasts input events (button presses, fader changes, MIDI) to all apps
- **Command Channel**: Apps send hardware commands (set LED color, configure CV, send MIDI)
- **Watch Channels**: Global configuration and clock synchronization
- **Serialization**: Postcard format for efficient no_std data exchange

## Getting Started

### Prerequisites

#### Hardware
- Raspberry Pi Pico 2 (RP2350B)
- Faderpunk PCB with supporting components
- USB cable for programming and power

#### Software
- Rust toolchain (nightly recommended)
- `picotool` for UF2 conversion
- Chromium-based browser for WebUSB configurator

### Development Environment

You will need:
- `rustup`
- Rust (1.89 or newer) with `thumbv8m.main-none-eabihf` target (`rustup target add thumbv8m.main-none-eabihf`)
- [picotool](https://github.com/raspberrypi/picotool)

## Building and Flashing

### Build Firmware

```bash
cd faderpunk # important, not in root
cargo build --release
```

### Create UF2 File

Use the provided script to build and convert to UF2 format:

```bash
# this needs to be done in the repository root
./build-uf2.sh
```

This creates `target/thumbv8m.main-none-eabihf/release/faderpunk.uf2`

### Flash to Device

1. Hold the SHIFT button (the one on the very bottom right, the yellow one) while connecting Faderpunk to your computer via USB.
2. Device appears as a mass storage device
3. Copy `faderpunk.uf2` to the device
4. Device automatically reboots with new firmware

### Development Workflow

For rapid development with debug output:

```bash
cd faderpunk
cargo build
# Use probe-rs or similar tool for flashing with RTT debug output
```

## Web Configurator

The Faderpunk Configurator is a React/TypeScript web application that communicates with the device via WebUSB.

### Features
- Drag-and-drop layout management
- Real-time parameter configuration
- Global settings (MIDI, I2C, clock, quantizer)
- Scene management
- Live visual feedback

### Running the Configurator


You will need:
- [NodeJS v22.x or higher](https://nodejs.org/en/download)
- [pnpm](https://pnpm.io)

Before building the configurator you'll need to run

```bash
# in the root
./gen-bindings.sh
```

This will create the Postcard bindings for the configurator (from libfp). 

```bash
cd configurator
pnpm install
pnpm dev
```

Access at `http://localhost:5173`

**NOTE:** When changes to libfp happen, the bindings will need to be regenerated. Delete the node_modules folder in configurator and do the following steps in that case.

### Browser Requirements
WebUSB requires:
- Chrome/Chromium
- Edge
- Opera
- Brave
- Vivaldi
or any Chromium-based browser

Firefox and Safari do not support WebUSB.

For more details, see [configurator/README.md](configurator/README.md)

## Development

### Project Structure

```
faderpunk/
├── faderpunk/           # Main firmware crate
│   ├── src/
│   │   ├── main.rs      # System initialization, core orchestration
│   │   ├── app.rs       # App API and abstractions
│   │   ├── apps/        # App implementations
│   │   ├── tasks/       # Hardware driver tasks
│   │   ├── events.rs    # Event types
│   │   ├── storage.rs   # FRAM persistence
│   │   └── layout.rs    # Channel layout management
│   └── Cargo.toml
├── libfp/               # Shared library
│   ├── src/             # Common types and utilities
│   └── Cargo.toml
├── configurator/        # Web configurator
│   ├── src/
│   └── package.json
├── gen-bindings/        # TypeScript binding generator
└── Cargo.toml           # Workspace configuration
```

### Creating a New App

1. Create a new file in `faderpunk/src/apps/my_app.rs`:


```rust
use embassy_futures::select::select;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use libfp::{AppIcon, Brightness, Color, Config, Range};
use crate::app::{App, Led};

pub const CHANNELS: usize = 1;

// App configuration visible to the configurator
pub static CONFIG: Config<0> = Config::new(
    "My App",
    "Description of what this app does",
    Color::Blue,
    AppIcon::Fader,
);

// Wrapper task - required for all apps
#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    // Exit handler allows clean shutdown when app is removed from layout
    select(run(&app), app.exit_handler(exit_signal)).await;
}

// Main app logic
pub async fn run(app: &App<CHANNELS>) {
    let output = app.make_out_jack(0, Range::_0_10V).await;
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    leds.set(0, Led::Button, Color::Blue, Brightness::Mid);

    loop {
        buttons.wait_for_down(0).await;
        let value = fader.get_value();
        output.set_value(value);
        leds.set(0, Led::Top, Color::Blue, Brightness::Custom((value / 16) as u8));
    }
}
```

2. Register in `faderpunk/src/apps/mod.rs`

```rust
    42 => my_app,
```

3. Build the firmware and see if you can find your app in the configurator

### Code Quality

```bash
# Check compilation
cargo check

# Run Clippy linter
cargo clippy

# Format code
cargo fmt
```

### Debugging

The project uses `defmt` for structured logging:

```rust
defmt::info!("Button pressed on channel {}", channel);
```

View logs using probe-rs or similar RTT-capable debugger.

## Storage and Scenes

Faderpunk uses a 1 Mbit FRAM (FM24V10) for persistent storage:

- Fast writes (no wear leveling needed)
- Scene-based storage system
- Apps can save/load state via serialization
- Global configuration persistence

Apps implement scene save/load by serializing their state with `postcard`.

## Communication Protocols

### MIDI
- USB MIDI device (shows as MIDI interface to host)
- Configurable channel routing
- Full MIDI message support via `midly` crate

### I2C
- 16n faderbank protocol support
- Eurorack module integration
- Configurable addressing

### WebUSB
- COBS framing for reliable packet transmission
- Postcard serialization for compact binary protocol
- Type-safe message definitions shared between firmware and configurator
- Real-time bidirectional communication

## Build Configuration

### Release Profile
The workspace Cargo.toml configures aggressive optimization:

```toml
[profile.release]
lto = true
incremental = false
codegen-units = 1
debug = 2
```

This maximizes performance while retaining debug symbols for profiling.

## Contributing

Contributions are welcome! Please follow the [Rust Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct).

### Development Guidelines
- Use `cargo fmt` and `cargo clippy` before committing
- Test on hardware when possible
- Document new apps and features
- Update TypeScript bindings when changing protocol types

### Pull Request Process
1. Fork the repository
2. Create a feature branch
3. Make your changes with clear commit messages
4. Ensure code passes clippy and builds successfully (warnings are, for the most part ok and expected at this point)
5. Test on hardware if applicable
6. Submit a pull request with a clear description

## Release Process

Faderpunk uses **knope** for release management on a single `main` branch:
- **Stable releases**: Automated via release PR flow
- **Beta releases**: Triggered manually via GitHub Actions `workflow_dispatch`

### Making a Stable Release

Stable releases are fully automated:

1. **Push commits to `main`** using [conventional commits](https://www.conventionalcommits.org/) (e.g., `feat:`, `fix:`)
2. **Release PR auto-created**: The `prepare-release` workflow creates/updates a `release` branch and PR with version bumps and changelogs
3. **Review the release PR**: Edit changelogs if needed, verify version bumps
4. **Merge the release PR**: This triggers the `release` workflow which:
   - Builds firmware (ELF + UF2) if faderpunk version changed
   - Builds configurator if configurator version changed
   - Creates GitHub releases with assets via `knope release`
   - Deploys configurator to GitHub Pages
   - Publishes `libfp` to crates.io if version changed

### Making a Beta Release

Beta releases are triggered manually:

1. **Go to Actions → Beta Release → Run workflow** on GitHub
2. The workflow:
   - Runs `knope prepare-release --prerelease-label beta` to bump versions (e.g., `1.8.0-beta.0`)
   - Commits and pushes the version bump to `main`
   - Builds firmware and configurator
   - Creates GitHub prereleases via `knope release`
   - Deploys configurator to `/beta/` on GitHub Pages
3. **Subsequent dispatches** increment the beta number (`beta.1`, `beta.2`, etc.)
4. **Merging a stable release PR** resets to the next stable version

### Making a Patch Release (Hotfix)

1. Create a branch from `main`, make the fix with a `fix:` commit, open a PR to `main`
2. After merging, the `prepare-release` workflow auto-updates the release PR with a patch bump
3. Merge the release PR to publish

### Release Artifacts

Each release produces:

**Firmware** (`faderpunk/`):
- `faderpunk.elf` - ELF binary for debugging
- `faderpunk-vX.Y.Z.uf2` - UF2 file for flashing to device

**Configurator** (`configurator/`):
- `configurator.zip` - Downloadable web app bundle
- GitHub Pages deployment (root for stable, `/beta` for beta)

**Library** (`libfp/` - stable only):
- Published to crates.io when version changes

## License

This project is licensed under **GNU General Public License v3.0** (GPL-3.0).

See [LICENSE](LICENSE) for full terms.

## Credits

**Faderpunk** is developed by [ATOV](https://atov.de)

- UI/UX by [Leise St. Clair](https://github.com/estcla)
- Icon design by [papernoise](https://www.papernoise.net)

## Support

- **Community**: [Discord Server](https://atov.de/discord)
- **Issues**: [GitHub Issues](https://github.com/ATOVproject/faderpunk/issues)
- **Website**: [atov.de](https://atov.de)

## Technical Specifications

### Performance
- Dual-core async execution
- Sub-millisecond event response
- 12-bit CV resolution

### Connectivity
- USB 1.1 (device and host)
- I2C (configurable speed)
- MIDI (USB and Serial)

### Power
- USB-powered
- Eurorack power compatible

---

Built with embedded Rust, Embassy async runtime, and a passion for modular synthesis.
