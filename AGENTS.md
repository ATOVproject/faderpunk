# AGENTS.md

Guidance for coding agents (Claude Code, etc.) and humans working in this repo.

> This file is the single source of truth for agent/contributor conventions.
> `CLAUDE.md` is a symlink to this file — edit `AGENTS.md`, never the symlink.

## Project Overview

Faderpunk is an embedded Rust eurorack/MIDI synthesizer controller running on RP2350B (Raspberry Pi Pico 2). It provides 16 channels with faders, buttons, RGB LEDs, and CV jacks, each capable of running independent "apps" (LFOs, sequencers, MIDI converters, etc.). Configuration is done via a React/TypeScript Web MIDI interface (SysEx on a dedicated virtual MIDI cable).

**Key Technologies:**
- Embedded Rust with `no_std`
- Embassy async runtime (dual-core)
- RP2350B microcontroller (overclocked to 250MHz)
- MAX11300 programmable mixed-signal I/O
- FM24V10 FRAM for persistent storage
- Config protocol over MIDI SysEx on virtual cable 2 (7-bit-packed Postcard)
- React 19 + Vite + Zustand for configurator

## Git workflow (every task)

Follow this for **every** task, in order. Do not skip steps.

1. **Branch off current `origin/main`.** The default/integration branch is
   `main` (CI, releases, and prepare-release all target it). Run `git fetch
   origin` first, then branch from `origin/main` with a consistent name
   (`feat/short-slug`, `fix/short-slug`). Never branch off stale local state.
   - Note: `origin/HEAD` may point at the stale `develop` branch — ignore it.
     `main` is the base for all new work.

2. **Implement, then double-check the relevant gates below before stopping.**
   These mirror CI (`.github/workflows/ci.yml`), so green here means green in CI.
   Run only the gates your change touches, plus formatting:

   **Rust (firmware / `libfp`)** — run from the repo root:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --bin faderpunk --target thumbv8m.main-none-eabihf -- -D warnings
   cargo clippy -p libfp -- -D warnings
   cargo test --lib -p libfp
   ```

   **Configurator** — if `libfp` protocol types changed, regenerate bindings
   first (`./gen-bindings.sh` from root), then:
   ```bash
   direnv exec . pnpm -C configurator lint
   direnv exec . pnpm -C configurator build   # build also type-checks (tsc -b)
   ```

   Then **STOP.** Wait for the user to review and adjust the code. Do not commit.

3. **On the user's go: commit.** A single one-line message in
   conventional-commit format (`feat:`, `fix:`, `chore:`, `refactor:`, …,
   optionally scoped e.g. `feat(genseq):`). Commit with the system git user
   (commits are signed). Do **not** add a `Co-authored-by: Claude` trailer.

4. **After committing, ask again** before opening a PR.

5. **PR (via `gh`):** brief, lean description. No emojis. Title follows
   `(feat|fix): short description`. Include a test checklist **only** when it
   adds verification value:
   - **New app or app fix** → checklist to test that app's functionality (or
     just the fix) on **actual hardware**.
   - **Firmware/hardware fix** → checklist for how to confirm the fix on
     **actual hardware**.
   - **Configurator fix** → steps for how to confirm the fix in the
     **configurator** (browser + live device).
   - Anything else → no checklist.

## Environment & Toolchain

The toolchain (Rust nightly, Node, pnpm, picotool, probe-rs) is provided by
**devenv** via **direnv** (`devenv.nix`, `.envrc`). A plain shell does not have
it on PATH. Run project commands through the environment:

```bash
direnv exec . <command>      # preferred (needs `direnv allow` once)
devenv shell -- <command>    # equivalent fallback
```

The firmware targets `thumbv8m.main-none-eabihf` and uses nightly features
(`build-std`). `faderpunk/.cargo/config.toml` sets this target by default, so a
bare `cargo build` works **from inside `faderpunk/`**. From the repo root you
must pass `--bin faderpunk --target thumbv8m.main-none-eabihf` explicitly (this
is what CI does, and it works on stable since the package-local `.cargo/config`
is not in scope from root).

## Build Commands

### Firmware (Embedded Rust)

```bash
# Build firmware from the package dir (uses configured target + build-std)
cargo build --release            # run from faderpunk/

# Build firmware from root (CI-equivalent; stable, no build-std)
cargo build --bin faderpunk --release --target thumbv8m.main-none-eabihf

# Build the flashable UF2 (wraps the above + picotool, from root)
./build-uf2.sh                   # → target/thumbv8m.main-none-eabihf/release/faderpunk.uf2

# Format / lint (see Git workflow for the full CI-matching gate set)
cargo fmt --all
cargo clippy --bin faderpunk --target thumbv8m.main-none-eabihf -- -D warnings
```

### Configurator (Web App)

```bash
# REQUIRED before first build (and after any libfp protocol-type or apps change):
./gen-bindings.sh                # from repo root — generates TS types from libfp

pnpm -C configurator install
pnpm -C configurator dev         # dev server
pnpm -C configurator build       # production build (also type-checks via tsc -b)
pnpm -C configurator lint        # eslint
```

`./gen-bindings.sh` runs the `gen-bindings` crate (postcard-bindgen) to generate
TypeScript types from the Rust structs in `libfp`. Re-run it whenever protocol
types in `libfp` change. If types change and the bindings seem stale, delete
`configurator/node_modules`, re-run `./gen-bindings.sh`, then reinstall.

`./gen-bindings.sh` also regenerates `configurator/src/demo/catalog.ts` — the
static app list used by the configurator's simulator mode (`VITE_SIMULATOR`) —
by parsing each registered app's `CONFIG` directly out of
`faderpunk/src/apps/*.rs`. That file is gitignored and never hand-edited: it's
produced fresh on every `./gen-bindings.sh` run, which already happens in CI,
beta, and release builds, so the simulator catalog can't drift out of sync
with the actual apps. Run `./gen-bindings.sh` once before local configurator
dev/build, same as the protocol bindings.

### Library (Shared Types)

The `libfp` crate contains shared types and utilities used by both firmware and
configurator. Changes here require regenerating bindings (see above). `libfp`
has unit tests (`cargo test --lib -p libfp`) — run them when you touch it.

## Architecture

### Dual-Core Design

**Core 0 (Hardware Tasks):** Embassy async tasks managing hardware interfaces:
- `tasks/max.rs` - MAX11300 ADC/DAC communication
- `tasks/buttons.rs` - Button scanning with debouncing
- `tasks/leds.rs` - WS2812B LED control
- `tasks/midi.rs` - MIDI I/O (USB and DIN)
- `tasks/fram.rs` - FRAM storage operations
- `tasks/i2c.rs` - I2C communication (16n protocol)
- `tasks/configure.rs` - Config protocol (SysEx over USB MIDI cable 2)
- `tasks/clock.rs` - Clock generation and synchronization

**Core 1 (Application Logic):** Runs user-facing apps as Embassy tasks. Each app instance is a separate task receiving hardware events and sending commands.

### Communication Between Cores

- **Event PubSub** (`events::EVENT_PUBSUB`): Broadcasts input events (button presses, fader changes) from Core 0 to all apps on Core 1
- **Command Channels**: Apps send commands to hardware tasks via async channels:
  - `MAX_CHANNEL` - Control CV jacks (ADC/DAC configuration, set values)
  - `APP_MIDI_CHANNEL` - Send MIDI messages
  - `I2C_LEADER_CHANNEL` - Send I2C messages
- **Watch Channels**: Global state synchronization:
  - `LAYOUT_WATCH` - Current channel layout
  - `GLOBAL_CONFIG_WATCH` - Device configuration
  - `CLOCK_PUBSUB` - Clock events

### App System

Apps are registered using the `register_apps!` macro in `faderpunk/src/apps/mod.rs`. Each app must provide:

1. **CHANNELS constant**: Number of channels the app uses (1-16)
2. **CONFIG constant**: Metadata (name, description, color, icon, parameters)
3. **wrapper task**: Embassy task entry point with `#[embassy_executor::task(pool_size = 16/CHANNELS)]`
4. **run function**: Main async logic

Apps interact with hardware through the `App<N>` API (`faderpunk/src/app.rs`):
- `app.use_faders()` - Access fader values
- `app.use_buttons()` - Await button events
- `app.use_leds()` - Control RGB LEDs
- `app.make_out_jack()` - Configure CV output jack
- `app.make_in_jack()` - Configure CV input jack
- `app.make_gate_jack()` - Configure gate output
- `app.use_midi()` - Send/receive MIDI
- `app.use_clock()` - Subscribe to clock events

### Layout Management

The `LayoutManager` (`faderpunk/src/layout.rs`) handles dynamic app spawning/despawning:
- Apps can be added/removed from channels at runtime
- Each app has an exit signal for clean shutdown
- Layout changes are persisted to FRAM automatically
- The layout is represented as `[Option<(app_id, channels, layout_id)>; 16]`

### Storage System

FRAM storage (`faderpunk/src/storage.rs`) uses memory-mapped regions:
- Global config (0-320): Device settings
- Runtime state (320-384): Current scene, etc.
- Layout (384-512): Channel layout
- Calibration (512-1024): CV calibration data
- App storage (1024-122880): Per-app scene data (16 scenes per app)
- App params (122880-131072): App parameter storage

Apps can implement scene storage by implementing serialization with `postcard`. The `AppStorage` trait provides save/load functionality.

### Protocol Design

**Config Communication (MIDI SysEx):**
1. Messages are serialized using `postcard` (compact binary format)
2. Wrapped in SysEx frames on USB-MIDI virtual cable 2: `F0 7D 46 50 01 <7-bit-packed payload> F7` (codec shared between `libfp/src/sysex.rs` and `configurator/src/utils/sysex.ts` — keep in sync)
3. Type definitions in `libfp` are shared between firmware and configurator via generated bindings
4. Messages flow bidirectionally: configurator → firmware (commands) and firmware → configurator (state updates)
5. The device is a pure class-compliant USB-MIDI device (no vendor interfaces) so embedded USB MIDI hosts work — see `docs/usb-host-compatibility.md`

**Key Message Types:**
- `ConfigureMessage` - Device commands (set layout, parameters, global config)
- `TransportMessage` - App metadata, current state, responses

## Creating a New App

1. Create `faderpunk/src/apps/my_app.rs` with CHANNELS, CONFIG, wrapper task, and run function
2. Register in `faderpunk/src/apps/mod.rs`: `register_apps!(... 42 => my_app,)`
3. If adding new parameter types to CONFIG, update `libfp/src/lib.rs`
4. Run `./gen-bindings.sh` — regenerates both the protocol bindings and the
   simulator app catalog (`configurator/src/demo/catalog.ts`) so the new app
   shows up in simulator mode
5. Build firmware and flash to device
6. App will appear in configurator's app library

**App Development Pattern:**
```rust
pub const CHANNELS: usize = 1;

pub static CONFIG: Config<N> = Config::new(
    "App Name",
    "Description",
    Color::Blue,
    AppIcon::Fader,
);

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    select(run(&app), app.exit_handler(exit_signal)).await;
}

pub async fn run(app: &App<CHANNELS>) {
    let faders = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    // Main app logic loop
    loop {
        // ...
    }
}
```

## Configurator Architecture

Located in `configurator/` directory. Key files:
- `src/store.ts` - Zustand state (MIDI device, apps, layout, config)
- `src/utils/midi-protocol.ts` - Web MIDI (SysEx) communication layer
- `src/components/input/` - Parameter input components for different types
- `src/components/settings/` - Global settings panels

**State Flow:**
1. User connects device → Web MIDI port discovery probes with GetVersion → device sends current state
2. User modifies layout/params → state updates → message sent to device via SysEx
3. Device sends acknowledgment → UI updates accordingly

**Adding New Parameter UI:**
When adding new parameter types to apps, create a corresponding input component in `src/components/input/` and register it in the parameter renderer.

## Development Workflow

### Typical Development Cycle

1. **Make firmware changes** in `faderpunk/src/`
2. **Build UF2**: `./build-uf2.sh` (from root)
3. **Flash device**: Hold SHIFT button (bottom right yellow button), connect USB, copy UF2 file
4. **Test with configurator**: `pnpm -C configurator dev`

### When Changing Protocol Types

1. **Edit types** in `libfp/src/`
2. **Regenerate bindings**: `./gen-bindings.sh` (from root)
3. **Delete configurator cache** (if bindings seem stale): `rm -rf configurator/node_modules`
4. **Reinstall**: `pnpm -C configurator install`
5. **Update UI** if needed in `configurator/src/components/`

### Debugging

**Firmware:**
- Uses `defmt` for logging: `defmt::info!("Message")`, `defmt::warn!("Warning")`, etc.
- View logs with probe-rs or RTT-capable debugger
- Logging is zero-cost in release builds

**Configurator:**
- Use browser DevTools console for config protocol debugging
- State inspection via React DevTools
- A MIDI monitor (e.g. `aseqdump -p <port>` on Linux) shows the raw SysEx traffic on the config port

## Important Patterns

### Memory Management
- The firmware is `no_std` - no heap allocator
- Use `heapless::Vec` and stack allocation
- Embassy provides `StaticCell` for static initialization
- Be mindful of stack sizes (Core 1 has 131KB stack)

### Concurrency
- All I/O is async via Embassy
- Use `select()` for concurrent operations
- PubSub for broadcast events
- Channels for point-to-point messaging
- Watch channels for shared state

### Parameter Storage
Apps can store parameters using `AppParams<N>` in CONFIG. Parameters are:
- Serialized with postcard
- Stored in FRAM per scene
- Automatically synced via the config protocol
- Type definitions shared with configurator

### Safety Considerations
- Hardware tasks use atomic operations (`portable_atomic`) for shared state
- No dynamic allocation
- All unsafe code is isolated to hardware drivers
- Critical sections protect interrupt-context access

## Release Process

Faderpunk uses **knope** for release management on a single `main` branch. Managed by GitHub Actions workflows.

- **Stable releases**: Push to `main` → `prepare-release` workflow creates/updates a release PR → merge PR → `release` workflow builds, tags, and deploys
- **Beta releases**: Manual `workflow_dispatch` on `beta` workflow → bumps versions with `-beta.N` suffix, builds, creates prereleases, deploys to `/beta/`

Configuration is in `knope.toml`. Three packages: `faderpunk`, `libfp`, `configurator` with independent versioning. Tags use `{package}/v{version}` format.

See README.md Release Process section for detailed workflows.

## Testing

`libfp` has unit tests — run `cargo test --lib -p libfp` (CI does). The firmware
itself has no automated test suite (Embassy tasks can't run in a standard test
harness); it is verified manually on hardware. The configurator is tested
manually in a Chromium browser with a live device connection.

## Common Pitfalls

1. **Branching off stale state**: Always `git fetch origin` and branch off `origin/main`. Ignore `origin/HEAD`/`develop`.
2. **Building firmware from root without flags**: From root you must pass `--bin faderpunk --target thumbv8m.main-none-eabihf`; a bare `cargo build` only works from inside `faderpunk/`.
3. **Forgetting bindings**: Run `./gen-bindings.sh` after changing libfp types or adding/editing an app's CONFIG — it also regenerates the gitignored simulator catalog (`configurator/src/demo/catalog.ts`).
4. **App pool sizes**: `pool_size = 16/CHANNELS` in task macro must match CHANNELS constant.
5. **Exit signals**: Apps must use `select(run(), exit_handler())` pattern for clean shutdown.
6. **FRAM address ranges**: Don't overlap storage regions in `storage.rs`.
7. **Atomic ordering**: Use `Ordering::Relaxed` for non-synchronized state, `Acquire`/`Release` for synchronized.
8. **Web MIDI requirements**: Browser must support Web MIDI with SysEx (Chromium, Firefox); HTTPS required for non-localhost. The user must grant the MIDI/SysEx permission.
9. **Commit trailers**: One-line conventional-commit messages; do not add a `Co-authored-by: Claude` trailer.

## File Structure Summary

```
faderpunk/           # Main firmware crate (embedded Rust)
├── src/
│   ├── main.rs      # Core initialization, dual-core setup
│   ├── app.rs       # App API and hardware abstractions
│   ├── apps/        # App implementations (20+ apps)
│   ├── tasks/       # Hardware driver tasks (Core 0)
│   ├── events.rs    # Event definitions and PubSub
│   ├── layout.rs    # Layout management and app spawning
│   ├── storage.rs   # FRAM persistence layer
│   └── macros.rs    # register_apps! macro

libfp/               # Shared library (no_std)
├── src/
│   ├── lib.rs       # Core types (Layout, Config, Params)
│   ├── types.rs     # Protocol message types
│   ├── sysex.rs     # Config-over-SysEx codec (mirrored in configurator)
│   └── quantizer.rs # Musical quantization

configurator/        # Web configurator (React + TypeScript)
├── src/
│   ├── App.tsx      # Main app component
│   ├── store.ts     # Zustand state management
│   ├── utils/midi-protocol.ts  # Web MIDI (SysEx) communication
│   ├── utils/sysex.ts          # SysEx codec (mirror of libfp/src/sysex.rs)
│   └── components/  # UI components

gen-bindings/        # TypeScript binding generator
├── src/main.rs      # Uses postcard-bindgen to generate TS types
└── src/catalog.rs   # Parses faderpunk/src/apps/*.rs to generate the
                      # simulator app catalog (configurator/src/demo/catalog.ts)
```
