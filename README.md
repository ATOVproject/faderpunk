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

    leds.set(0, Led::Button, Color::Blue, Brightness::Lower);

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

Faderpunk uses a dual-track release system managed by release-please:
- **Beta releases**: Published from the `develop` branch (e.g., `1.6.0-beta.5`)
- **Stable releases**: Published from the `main` branch (e.g., `1.5.0`)

Both workflows are automated via GitHub Actions, but version management requires manual steps to keep the tracks synchronized.

### Making a Beta Release

Beta releases happen automatically when commits are merged to the `develop` branch:

1. **Merge changes to `develop`**:
   ```bash
   git checkout develop
   git merge feature-branch
   git push origin develop
   ```

2. **Release-please creates a PR**:
   - Workflow runs automatically on push
   - Creates/updates a release PR with changelog
   - Review the PR to verify version bumps and changelog

3. **Merge the release PR**:
   - Merge the release-please PR on GitHub
   - This triggers the build and publish workflow
   - Beta releases are published with `prerelease: true` flag
   - Configurator deploys to GitHub Pages at `/beta` path

### Making a Stable Release

Stable releases happen when `develop` is ready for production:

1. **Create PR from `develop` to `main`**:
   ```bash
   git checkout develop
   git push origin develop  # Ensure develop is up to date
   ```
   Then create a PR on GitHub from `develop` → `main`

2. **Review and merge to `main`**:
   - Review the PR carefully
   - Merge to `main` when ready for stable release

3. **Release-please creates a release PR on `main`**:
   - Workflow runs automatically
   - Creates/updates a release PR with changelog
   - Version numbers will match what was in develop

4. **Merge the release PR**:
   - Merge the release-please PR on GitHub
   - This triggers the build and publish workflow
   - Stable releases are published as full releases (not prereleases)
   - Configurator deploys to GitHub Pages root path
   - `libfp` is published to crates.io (if version changed)

### Critical: Sync Branches After Stable Release

**IMPORTANT**: After a stable release is published, you must sync the release history back to `develop` and bump beta versions ahead of stable.

5. **Merge `main` back into `develop`**:
   ```bash
   git checkout develop
   git pull origin develop
   git merge main --no-edit
   git push origin develop
   ```

   This ensures release-please on `develop` sees the stable release commits and doesn't get confused about what's been released.

6. **Bump beta versions ahead of stable**:

   Edit `.release-please-manifest.beta.json` to increment the minor version and reset to `-beta.0`:

   ```json
   {
     "faderpunk": "1.6.0-beta.0",
     "configurator": "1.7.0-beta.0"
   }
   ```

   For example, if stable just released `faderpunk-1.5.0`, beta should jump to `1.6.0-beta.0`.

7. **Commit and push the version bump**:
   ```bash
   git add .release-please-manifest.beta.json
   git commit -m "chore: bump beta versions ahead of stable"
   git push origin develop
   ```

### Version Management Rules

- Beta versions must always be ahead of the latest stable release
- Use semantic versioning: `MAJOR.MINOR.PATCH` for stable, `MAJOR.MINOR.PATCH-beta.N` for beta
- When stable releases `X.Y.0`, beta should jump to `X.(Y+1).0-beta.0`
- The merge from `main` to `develop` is required for release-please to track what's been released

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
