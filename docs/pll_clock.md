Here is the finalized design document for our refactor. You can copy this directly into your `docs/plans/` directory.

---

# Design Document: Unified Clock Engine Refactor

**Date:** 2026-02-25
**Topic:** Clock Architecture Refactor (PLL Foundation)

## Overview

This document outlines the architecture for refactoring the MIDI controller's clock system. The goal is to replace the current competing clock sources with a single, unified tick generator. This refactor resolves existing technical debt and lays the mathematical and structural groundwork for future features, specifically clock upscaling (generating 24 PPQN from lower resolution external clocks) and swing/shuffle.

## Architecture & Data Flow

The system shifts from multiple independent tick generators to a sensor-and-engine model.

* **Hardware Sensors:** The external pin loops (`atom`, `meteor`, `cube`) no longer generate `ClockInEvent::Tick` messages. Instead, they act as high-precision interrupt sensors. Upon detecting a falling edge, they capture `Instant::now()` and emit a `HardwarePulse` event containing the timestamp and source.
* **Unified Clock Engine:** A single central task replaces `internal_fut`. This engine is the exclusive generator of ticks in the system.
* **Internal Mode:** Generates ticks purely based on the BPM defined in `GlobalConfig`.
* **External Mode:** Pauses its internal free-running timer and acts on incoming `HardwarePulse` events.


* **Gatekeeper:** The existing `run_clock_gatekeeper` task remains largely unmodified. It continues to handle routing ticks to MIDI outs and analog jacks, unaware that external ticks are now passing through a central engine.

## Delta Measurement & Phase Alignment

To support future upscaling without introducing latency, the engine separates phase correction from frequency measurement.

* **Immediate Phase Reaction (The Snap):** When a `HardwarePulse` is received from the active external source, the engine immediately dispatches a `Tick` to the gatekeeper and resets its internal phase timer. This guarantees zero-latency synchronization with external gear.
* **Smoothed Frequency Calculation (The Prediction):** In the background, the engine calculates the time delta between the current and previous `HardwarePulse`.

To filter out electrical jitter, it maintains a small rolling average (e.g., last 3-4 pulses) to determine the highly accurate `current_tick_duration`. While acting as a 1:1 pass-through for now, this smoothed duration is the exact metric required for future intermediate tick synthesis.

## Error Handling & Edge Cases

The engine implements safeguards to protect the controller's stability from unpredictable hardware behavior.

* **Hardware Debouncing:** To prevent electrical ringing from destroying the tempo calculation, the engine enforces a minimum delta threshold. Any `HardwarePulse` arriving impossibly fast (e.g., representing > 300 BPM) is discarded as noise.
* **Clock Dropout Watchdog:** If an active external clock is unexpectedly disconnected or stops, the engine utilizes a dynamic timeout window (e.g., 3x the current smoothed `current_tick_duration`). If no pulse is received within this window, the external clock is flagged as lost, and the transport is cleanly halted.
* **Seamless Source Switching:** When the user changes the `ClockSrc` in the global config (e.g., from an external drum machine back to `Internal`), the engine instantly drops the external listeners and resumes its internal timer relative to the *last valid tick* fired, preventing abrupt phase jumps or stutters during the handoff.

---

That really is a brilliant catch. If MIDI clock bypasses the engine, we completely lose the ability to apply swing, upscaling, or any timing adjustments to it. By funneling *everything* through the engine, MIDI becomes just another high-precision sensor.

This also means the unified engine will act as the ultimate funnel for external transport commands (Start/Stop) as well, ensuring its internal `is_running` state is always perfectly synced with whatever the gatekeeper is doing.

Here is the updated, comprehensive implementation plan, incorporating `midi.rs`.

### Phase 1: Data Structures & Channels Definition

We will create a new communication layer that feeds the Engine, leaving the existing `CLOCK_IN_CHANNEL` strictly for Engine -> Gatekeeper communication.

* **Define `SyncEngineEvent`:** In `tasks::clock`, create a new enum to represent all inputs to the engine.
* `Pulse { source: ClockSrc, timestamp: Instant }` (For analog ticks and MIDI TimingClock).
* `Transport(ClockInEvent)` (For external Start/Stop/Continue/Reset).


* **Create `SYNC_ENGINE_CHANNEL`:** A new channel (size 16) to route events from the analog pins and `midi.rs` to the Engine.
* **Constants:** Add `DEBOUNCE_THRESHOLD` (e.g., 8ms) and `HISTORY_SIZE` (e.g., 4) to `clock.rs`.

---

### Phase 2: Refactoring the Sensors (Analog & MIDI)

We will strip the decision-making logic out of the sensor tasks so they simply report events blindly.

* **Update `midi.rs` (`process_midi_event`):** * Change the `clock_in_sender` argument to `sync_engine_sender`.
* Map `SystemRealtime::TimingClock` -> `SyncEngineEvent::Pulse { source: clock_src, timestamp: Instant::now() }`.
* Map Start/Stop/Continue/Reset -> `SyncEngineEvent::Transport(ClockInEvent::...)`.


* **Update `clock.rs` (`make_ext_clock_loop`):**
* Remove `GLOBAL_CONFIG_WATCH`. The sensor no longer needs to know if it's the active clock or reset source.
* Wait for falling edge -> capture `Instant::now()` -> send `SyncEngineEvent::Pulse`.
* *(Note: The Engine will now handle figuring out if that analog pulse represents a Clock Tick or a Reset based on the config!)*



---

### Phase 3: Building the Unified Engine Task

Create the new `run_unified_clock_engine` task in `clock.rs`. This task is the "brain" and the exclusive sender to `CLOCK_IN_CHANNEL`.

* **State Initialization:**
* `is_running`, `last_pulse`, `delta_history` (array), `history_idx`.
* `current_tick_duration` (derived from `Internal` BPM on boot).
* `next_tick_at`.


* **The Select Loop (4 Arms):**
1. **Config Changes (`config_receiver.changed()`):** Update active sources. If switching to `Internal`, reset `last_pulse`, clear history, and recalculate `current_tick_duration`.
2. **Internal Transport (`TRANSPORT_CMD_CHANNEL`):** Handle UI Start/Stop commands. Update `is_running`. If starting on `Internal`, schedule the immediate first tick.
3. **External Sync Events (`SYNC_ENGINE_CHANNEL`):**
* *If `Transport`:* Validate against active source. Update `is_running` and forward to `CLOCK_IN_CHANNEL`.
* *If `Pulse`:* * Check if the source matches `config.clock.clock_src` (treat as tick) or `config.clock.reset_src` (treat as reset and forward).
* If it's a tick: Apply `DEBOUNCE_THRESHOLD`.
* **Phase Snap:** Send `ClockInEvent::Tick` to `CLOCK_IN_CHANNEL` immediately. Update `next_tick_at` to prevent internal timer from popping.
* **Frequency Math:** Calculate delta, push to history array, average it, and store in `current_tick_duration`. Update `last_pulse`.




4. **Internal Timer (`Timer::at(next_tick_at)`):**
* Only awaits if `is_running`.
* *If Internal Source:* Fire `ClockInEvent::Tick`, advance `next_tick_at` by `current_tick_duration`.
* *If External Source (Watchdog):* If this fires, it means an external pulse was missed (dropout). Halt transport, clear `last_pulse`, and send a Stop event.





---

### Phase 4: Integration & Wiring

Wire up the new architecture and clean up the old one.

* **Update `run_clock_sources`:**
* Delete the old `internal_fut` block.
* Spawn the three simplified analog `make_ext_clock_loop` futures.
* Spawn the new `run_unified_clock_engine` future.


* **Update `midi_in_task` (in `midi.rs`):**
* Pass the new `SYNC_ENGINE_CHANNEL.sender()` into `process_midi_event` instead of the old clock sender.


* **Verify `run_clock_gatekeeper`:** Ensure it remains untouched, continuing to route ticks to analog jacks and MIDI outputs.

---

This plan fully unifies the architecture and ensures MIDI clock will be perfectly positioned for swing and upscaling down the line.


CLAUDE PLAN

 Unified Clock Engine Refactor

 Context

 The current clock system has competing tick generators: an internal timer (internal_fut), 3 external pin loops, and MIDI inputs - all independently sending events to CLOCK_IN_CHANNEL. The
 gatekeeper filters by active source. This makes future clock upscaling and swing/shuffle impossible because there's no central place to measure tempo or synthesize intermediate ticks.

 The refactor creates a sensor-and-engine model: external pins and MIDI become dumb pulse/event reporters into a new SYNC_ENGINE_CHANNEL, and a single Unified Clock Engine becomes the exclusive
 sender to CLOCK_IN_CHANNEL. The gatekeeper remains unchanged.

 Files to Modify

 1. faderpunk/src/tasks/clock.rs - New types, simplified ext loops, unified engine, updated run_clock_sources
 2. faderpunk/src/tasks/midi.rs - Route MIDI realtime events through SYNC_ENGINE_CHANNEL instead of CLOCK_IN_CHANNEL

 Files That Stay Unchanged

 - run_clock_gatekeeper (still receives from CLOCK_IN_CHANNEL, publishes to CLOCK_PUBSUB)
 - All app clock consumption (app.rs, all apps using use_clock())
 - libfp types (ClockSrc, ClockConfig, GlobalConfig, etc.)
 - CLOCK_IN_CHANNEL, CLOCK_PUBSUB, TRANSPORT_CMD_CHANNEL definitions

 ---
 Step 1: New data structures and channel (clock.rs)

 Add after existing statics/constants:

 #[derive(Clone, Copy)]
 pub enum SyncEngineEvent {
     /// A timing pulse from an analog pin or MIDI TimingClock
     Pulse { source: ClockSrc, timestamp: Instant },
     /// A transport command from an external source (MIDI Start/Stop/Continue/Reset)
     Transport(ClockInEvent),
 }

 pub static SYNC_ENGINE_CHANNEL: Channel<ThreadModeRawMutex, SyncEngineEvent, 16> = Channel::new();

 const DEBOUNCE_THRESHOLD: Duration = Duration::from_millis(8); // ~312 BPM at 24 PPQN
 const HISTORY_SIZE: usize = 4;
 const WATCHDOG_MULTIPLIER: u32 = 3;

 Update imports to add select4 and Either4.

 ---
 Step 2: Simplify make_ext_clock_loop (clock.rs)

 Replace the current function (lines 112-151) with a dumb pulse reporter:

 async fn make_ext_clock_loop(mut pin: Input<'_>, clock_src: ClockSrc) {
     let sender = SYNC_ENGINE_CHANNEL.sender();
     loop {
         pin.wait_for_falling_edge().await;
         pin.wait_for_low().await;
         sender.send(SyncEngineEvent::Pulse {
             source: clock_src,
             timestamp: Instant::now(),
         }).await;
     }
 }

 Removes: GLOBAL_CONFIG_WATCH access, CLOCK_IN_CHANNEL access, reset_src logic, config-based active/inactive filtering. The engine handles all routing.

 ---
 Step 3: Update MIDI to send through engine (midi.rs)

 Import change (line 30):
 // Before:
 use crate::tasks::clock::{ClockInEvent, CLOCK_IN_CHANNEL};
 // After:
 use crate::tasks::clock::{ClockInEvent, SyncEngineEvent, SYNC_ENGINE_CHANNEL};

 Add Instant to embassy_time import (line 16).

 midi_in_task (line 330):
 // Before:
 let clock_in_sender = CLOCK_IN_CHANNEL.sender();
 // After:
 let sync_engine_sender = SYNC_ENGINE_CHANNEL.sender();

 Pass sync_engine_sender instead of clock_in_sender to both process_midi_event calls (lines 412, 442).

 process_midi_event (line 494): Change parameter type and body:
 async fn process_midi_event(
     event: &LiveEvent<'_>,
     publisher: &MidiPubSubPublisher,
     thru_targets: [bool; 3],
     clock_src: ClockSrc,
     sync_engine_sender: &Sender<'static, ThreadModeRawMutex, SyncEngineEvent, 16>,
     midi_sender: &Sender<'static, CriticalSectionRawMutex, MidiOutEvent, 16>,
 ) {
     match event {
         LiveEvent::Realtime(msg) => match msg {
             SystemRealtime::TimingClock => {
                 sync_engine_sender.send(SyncEngineEvent::Pulse {
                     source: clock_src,
                     timestamp: Instant::now(),
                 }).await;
             }
             SystemRealtime::Start => {
                 sync_engine_sender.send(SyncEngineEvent::Transport(
                     ClockInEvent::Start(clock_src)
                 )).await;
             }
             SystemRealtime::Stop => {
                 sync_engine_sender.send(SyncEngineEvent::Transport(
                     ClockInEvent::Stop(clock_src)
                 )).await;
             }
             SystemRealtime::Continue => {
                 sync_engine_sender.send(SyncEngineEvent::Transport(
                     ClockInEvent::Continue(clock_src)
                 )).await;
             }
             SystemRealtime::Reset => {
                 sync_engine_sender.send(SyncEngineEvent::Transport(
                     ClockInEvent::Reset(clock_src)
                 )).await;
             }
             _ => {}
         },
         // ... non-realtime handling unchanged
     }
 }

 ---
 Step 4: Build the Unified Engine (clock.rs)

 New async function (replaces internal_fut):

 async fn run_unified_clock_engine() {

 State:
 - is_running: bool - from is_clock_running().await
 - last_pulse: Option<Instant> - last valid external pulse timestamp
 - delta_history: [Duration; HISTORY_SIZE] - rolling average buffer (init to zero)
 - history_idx: usize - circular index
 - current_tick_duration: Duration - from bpm_to_clock_duration() initially
 - next_tick_at: Instant - next internal tick / watchdog deadline
 - config: GlobalConfig - snapshot from config watch

 Channels:
 - clock_in_sender = CLOCK_IN_CHANNEL.sender() (engine is exclusive sender)
 - sync_engine_receiver = SYNC_ENGINE_CHANNEL.receiver()
 - transport_receiver = TRANSPORT_CMD_CHANNEL.receiver()
 - config_receiver = GLOBAL_CONFIG_WATCH.receiver()

 Startup: If is_running and source is Internal, send ClockInEvent::Start(Internal) and schedule first tick with TICK_RESET_DELAY.

 Main loop with select4:

 Arm 1: Config changes

 - If clock_src changed: reset last_pulse, clear delta_history/history_idx
 - If switching to Internal: recalculate current_tick_duration from BPM, schedule next_tick_at
 - If BPM changed while Internal + running: proportionally rescale next_tick_at (preserve existing logic from internal_fut lines 367-390)

 Arm 2: Transport commands (from UI buttons via TRANSPORT_CMD_CHANNEL)

 - Only effective when clock_src == Internal
 - Same logic as current internal_fut (lines 392-417): toggle is_running, send Start/Stop to CLOCK_IN_CHANNEL, persist via store_clock_running()

 Arm 3: Sync engine events (from SYNC_ENGINE_CHANNEL)

 For SyncEngineEvent::Transport(event):
 - Validate: event source must match config.clock.clock_src
 - Forward the ClockInEvent directly to CLOCK_IN_CHANNEL
 - Update is_running based on Start/Stop/Continue

 For SyncEngineEvent::Pulse { source, timestamp }:
 - Check if source matches config.clock.reset_src → send ClockInEvent::Reset(source) to CLOCK_IN_CHANNEL, done
 - Check if source matches config.clock.clock_src → process as clock tick:
   a. Debounce: If last_pulse exists and timestamp - last_pulse < DEBOUNCE_THRESHOLD, discard
   b. Phase snap: Send ClockInEvent::Tick(source) to CLOCK_IN_CHANNEL immediately
   c. Frequency math: Calculate delta from last_pulse, push to delta_history[history_idx], increment history_idx % HISTORY_SIZE, compute rolling average of non-zero entries → update
 current_tick_duration
   d. Update last_pulse = Some(timestamp)
   e. Reschedule watchdog: next_tick_at = timestamp + current_tick_duration * WATCHDOG_MULTIPLIER
 - If matches neither: discard silently

 Arm 4: Timer (Timer::at(next_tick_at))

 The timer future's behavior depends on mode:
 - Internal + running: Fire tick → send ClockInEvent::Tick(Internal), advance next_tick_at += current_tick_duration
 - External + running + last_pulse.is_some(): This is the watchdog. If it fires, external clock is lost → send ClockInEvent::Stop(clock_src), clear last_pulse, set is_running = false, persist
 - Otherwise: core::future::pending() (don't fire)

 Timer scheduling: next_tick_at is updated by:
 - Internal tick handler: next_tick_at += current_tick_duration
 - External pulse handler: next_tick_at = timestamp + current_tick_duration * WATCHDOG_MULTIPLIER
 - Config change (BPM): proportional rescale
 - Transport start: next_tick_at = Instant::now() + TICK_RESET_DELAY

 ---
 Step 5: Update run_clock_sources (clock.rs)

 Replace lines 316-427:

 #[embassy_executor::task]
 async fn run_clock_sources(aux_inputs: AuxInputs) {
     let (atom_pin, meteor_pin, hexagon_pin) = aux_inputs;
     let atom = Input::new(atom_pin, Pull::Up);
     let meteor = Input::new(meteor_pin, Pull::Up);
     let cube = Input::new(hexagon_pin, Pull::Up);

     let engine_fut = run_unified_clock_engine();
     let atom_fut = make_ext_clock_loop(atom, ClockSrc::Atom);
     let meteor_fut = make_ext_clock_loop(meteor, ClockSrc::Meteor);
     let cube_fut = make_ext_clock_loop(cube, ClockSrc::Cube);

     join4(engine_fut, atom_fut, meteor_fut, cube_fut).await;
 }

 The old internal_fut closure is completely deleted.

 ---
 Step 6: Cleanup

 - Remove unused imports (select3, Either3 if no longer used in clock.rs)
 - Move Spawner usage into engine function for store_clock_running()
 - Verify CLOCK_PUBSUB_PUBLISHERS constant (currently 5) - may not need changing since the gatekeeper is the only publisher to CLOCK_PUBSUB, and the constant refers to that channel

 ---
 Verification

 1. cd faderpunk && cargo check - must compile
 2. cargo clippy - no new warnings
 3. cargo build --release - builds for thumbv8m target
 4. Manual hardware testing:
   - Internal clock start/stop via button toggle, BPM changes smooth
   - External analog clock: ticks pass through with phase snap
   - MIDI clock: TimingClock + Start/Stop/Continue work through engine
   - Source switching: Internal ↔ External, no phase jump
   - Clock dropout: disconnect external source → clean transport stop after watchdog
   - Reset source: analog pin configured as reset → reset events forwarded

# Further fixes

## Clock sync drift fix

### Context

When changing BPM on the internal clock (e.g., via the configurator fader), the clock drifts out of sync. The drift is worse with faster tempo changes. This is a phase accumulation error in the unified clock engine's BPM change handler.

### Root Cause

In `faderpunk/src/tasks/clock.rs` lines 387-401, when BPM changes, the code rescales the remaining time and rebases `next_tick_at` to `Instant::now()`:

```rust
let now = Instant::now();
// ...
next_tick_at = now + new_time_until_next_tick;
```

In contrast, the steady-state tick handler (line 510) advances the grid correctly:

```rust
next_tick_at += current_tick_duration;  // grid-relative, no Instant::now()
```

The problem: each BPM change rebases the tick grid to the current wall-clock time. `Instant::now()` includes processing delay, scheduler jitter, and the time between the config change arriving and the handler running. With rapid BPM changes (many config updates per tick period), these small errors compound, causing cumulative drift.

### Fix

Track `last_tick_at` and compute all tick scheduling relative to this grid anchor — never relative to `Instant::now()`.

- When BPM changes: `next_tick_at = last_tick_at + new_tick_duration` (recompute from the anchor).
- When a tick fires: `last_tick_at = next_tick_at` (record the ideal scheduled time, not actual).

This eliminates all jitter accumulation because:

- `last_tick_at` is always the ideal grid point (set from the scheduled time, not `Instant::now()`)
- Rapid BPM changes just recompute `next_tick_at` from the same anchor — no compounding
- No `Instant::now()` in the calculation path

If a BPM increase makes `next_tick_at` land in the past, `Timer::at` fires immediately — the tick handler naturally catches up by one tick and re-establishes the grid.
