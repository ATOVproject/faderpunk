# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is "Faderpunk" - an embedded Rust project for the RP2350B microcontroller that creates a eurorack-style synthesizer module with 16 channels of faders, buttons, and CV jacks.

## Architecture

### Core Structure
- **Dual-core execution**: Core 0 handles hardware tasks (MAX11300 ADC/DAC, buttons, LEDs, MIDI, FRAM), Core 1 runs application logic
- **App-based architecture**: Each channel can run a different "app" (like `default`, `lfo`) that defines behavior
- **Embassy framework**: Uses Embassy async runtime for embedded Rust with tasks and channels
- **Hardware abstraction**: Apps interact through the `App<N>` API rather than directly with hardware

### Key Components
- **`src/main.rs`**: System initialization, peripheral setup, dual-core orchestration
- **`src/app.rs`**: Core App API that provides hardware abstractions (buttons, faders, LEDs, CV jacks, MIDI, storage)
- **`src/apps/`**: Individual app implementations that define channel behavior
- **`src/tasks/`**: Hardware driver tasks (MAX11300, buttons, LEDs, MIDI, FRAM, etc.)

### Communication
- **Event PubSub**: Input events (button presses, fader changes, MIDI) broadcast to all apps
- **Command Channel**: Apps send hardware commands (set LED, send MIDI, configure CV jacks)
- **Watch channels**: Global configuration and clock signals

### Storage
- **FRAM-based persistence**: Apps can save/load state to/from FRAM using scenes
- **Serialization**: Uses postcard for efficient no_std serialization

## Development Commands

### Build and Flash
```bash
cargo build --release
```

### Check Code
```bash
cargo check
cargo clippy
```

### Development Environment
Uses Nix flake for development environment setup:
```bash
nix develop
```

## Working with Apps

Apps are the core abstraction - each represents behavior for one or more channels. Apps:
- Run as Embassy tasks on Core 1
- Receive a channel range and hardware abstractions through the `App<N>` API
- Can configure CV jacks, control LEDs, respond to buttons/faders
- Support scene save/load for state persistence

When creating new apps, follow the pattern in `src/apps/default.rs` and `src/apps/lfo.rs`.