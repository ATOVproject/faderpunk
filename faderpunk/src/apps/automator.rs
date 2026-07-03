use embassy_futures::{
    join::join5,
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use embassy_time::{with_timeout, Duration, Instant};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{attenuate, attenuate_bipolar, interp_loop_sample, midi_gate, slew_2, split_unsigned_value},
    AppIcon, Brightness, ClockDivision, Color, Config, Curve, MidiCc, MidiChannel, MidiOut, Param,
    Range, Value, APP_MAX_PARAMS,
};

use crate::{
    app::{
        App, AppParams, AppStorage, Arr, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
    },
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 7;

const MAX_BUFFER_SAMPLES: usize = 192;
const CLOCK_TIMEOUT_MS: u64 = 500;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Automator",
    "CV gesture looper",
    Color::Cyan,
    AppIcon::Fader,
)
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiCc { name: "MIDI CC" })
.add_param(Param::Range {
    name: "Range",
    variants: &[Range::_0_10V, Range::_Neg5_5V],
})
.add_param(Param::Color {
    name: "Color",
    variants: &[
        Color::Blue,
        Color::Green,
        Color::Rose,
        Color::Orange,
        Color::Cyan,
        Color::Pink,
        Color::Violet,
        Color::Yellow,
    ],
})
.add_param(Param::MidiNrpn)
.add_param(Param::MidiOut)
.add_param(Param::Enum {
    name: "Resolution",
    variants: &["1 ppqn / 2 bars", "2 ppqn / 4 bars", "4 ppqn / 8 bars"],
});

pub struct Params {
    midi_channel: MidiChannel,
    midi_cc: MidiCc,
    range: Range,
    color: Color,
    nrpn: bool,
    midi_out: MidiOut,
    resolution: usize,
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            midi_channel: MidiChannel::from_value(values[0]),
            midi_cc: MidiCc::from_value(values[1]),
            range: Range::from_value(values[2]),
            color: Color::from_value(values[3]),
            nrpn: bool::from_value(values[4]),
            midi_out: MidiOut::from_value(values[5]),
            resolution: usize::from_value(values[6]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.nrpn.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.resolution.into()).unwrap();
        vec
    }
}

/// Looper operating mode.
#[derive(Clone, Copy, PartialEq)]
enum LooperState {
    /// No loop. Fader = direct CV/MIDI output.
    Passthrough,
    /// Button held, waiting for 16th-note boundary before recording starts.
    PendingRecording,
    /// Recording live fader values into rec_buf each clock tick.
    Recording,
    /// Loop playing back. Fader = bipolar offset added to loop signal.
    Playing,
}

/// Commands issued by the button handler and consumed by the clock handler
/// at the next tick boundary, keeping all buffer mutations synchronised.
#[derive(Clone, Copy, PartialEq)]
enum LooperCmd {
    None,
    /// PendingRecording → Recording: held until next 16th-note boundary.
    PendingStart,
    /// Recording → Playing (or Passthrough if nothing captured): held until next 16th-note.
    PendingCommit,
    /// Erase the loop and return to Passthrough.
    ClearLoop,
    /// Scene loaded — reload play_buf from storage.
    LoadFromStorage,
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    /// The committed playback loop.
    buffer: Arr<u16, MAX_BUFFER_SAMPLES>,
    has_loop: bool,
    /// Number of valid samples in buffer.
    loop_len: u16,
    /// Attenuator level (0–4095, default = max).
    att_val: u16,
    /// Fader offset centre (0–4095, centre = 2048 = no offset).
    offset_val: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            buffer: Arr::default(),
            has_loop: false,
            loop_len: 0,
            att_val: 4095,
            offset_val: 2048,
        }
    }
}

impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16 / CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let ch = app.start_channel as u8;
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            midi_channel: MidiChannel::default(),
            midi_cc: MidiCc::from(32u8.saturating_add(ch)),
            range: Range::_0_10V,
            color: Color::Cyan,
            nrpn: false,
            midi_out: MidiOut::default(),
            resolution: 1,
        },
    );
    let storage = ManagedStorage::<Storage>::new(app.app_id, app.layout_id);

    param_store.load().await;
    storage.load().await;

    let app_loop = async {
        loop {
            select3(
                run(&app, &param_store, &storage),
                param_store.param_handler(),
                storage.saver_task(),
            )
            .await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(
    app: &App<CHANNELS>,
    params: &ParamStore<Params>,
    storage: &ManagedStorage<Storage>,
) {
    let (midi_out_cfg, midi_chan, midi_cc, range, nrpn, led_color, resolution) = params.query(|p| {
        (
            p.midi_out,
            p.midi_channel,
            p.midi_cc,
            p.range,
            p.nrpn,
            p.color,
            p.resolution,
        )
    });
    let clock_div = match resolution {
        0 => ClockDivision::_1,
        2 => ClockDivision::_4,
        _ => ClockDivision::_2,
    };

    let mut clock = app.use_clock();
    // Function pointer into TICK_COUNTER; used by clock_handler to check 16th-note boundaries.
    let tick_fn = clock.get_ticker();
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let midi = app.use_midi_output(midi_out_cfg, midi_chan, nrpn);
    let output = app.make_out_jack(0, range).await;
    let bipolar = range.is_bipolar();

    let state_glob = app.make_global(LooperState::Passthrough);
    let cmd_glob = app.make_global(LooperCmd::None);
    let clock_running_glob = app.make_global(false);
    // Shared att/offset: main_loop writes, clock_handler reads.
    let att_glob = app.make_global(storage.query(|s| s.att_val));
    let offset_glob = app.make_global(storage.query(|s| s.offset_val));
    let has_saved_loop = storage.query(|s| s.has_loop);
    // Last computed output for fader LED display.
    let last_output_glob = app.make_global(0u16);
    // Raw loop target written by clock_handler; main_loop interpolates and drives output.
    // Seeded with the first saved sample so output is stable before the clock starts.
    let seed = if has_saved_loop { storage.query(|s| s.buffer.at(0)) } else { 0u16 };
    let loop_prev_glob = app.make_global(seed);
    let loop_target_glob = app.make_global(seed);
    // Measured ms between the last two clock ticks; 0 = no tick seen yet.
    let tick_interval_glob = app.make_global(0u32);
    // Incremented by clock_handler on every Playing tick; main_loop detects
    // changes to reset elapsed_ms reliably, even when consecutive samples are equal.
    let tick_id_glob = app.make_global(0u8);
    if has_saved_loop {
        state_glob.set(LooperState::Playing);
        // Queue buffer load so it fires at the first clock tick rather than now.
        // This sets loop_start_tick correctly and prevents reloads on clock stop+start.
        cmd_glob.set(LooperCmd::LoadFromStorage);
        leds.set(0, Led::Button, Color::Green, Brightness::High);
    } else {
        leds.set(0, Led::Button, led_color, Brightness::Mid);
    }

    // ── Clock handler ────────────────────────────────────────────────────────
    // Advances the loop, writes/reads sample buffers, computes the per-sample
    // target and writes it to loop_target_glob, and sends MIDI CC.  All buffer
    // mutations happen here, at tick boundaries.
    // CV output is driven by main_loop, which slews loop_target_glob at 1 ms.
    //
    // PendingStart / PendingCommit are quantised to 16th-note boundaries:
    // 24 PPQN / 4 = 6 underlying ticks per 16th note.  tick_fn() % 6 == 0
    // detects this boundary regardless of the active clock_div.
    let clock_handler = async {
        // play_buf is intentionally absent: the authoritative buffer lives in
        // storage.inner.buffer (updated by PendingCommit and scene recall) and is
        // read per-tick via storage.query().  This keeps playback across run()
        // restarts (param changes) without any storage reload.
        let mut rec_buf = [0u16; MAX_BUFFER_SAMPLES];
        let mut rec_head: usize = 0;
        let mut loop_len: usize = storage.query(|s| s.loop_len as usize).max(1);
        let mut loop_start_tick: u32 = 0;
        let mut last_tick: Option<Instant> = None;

        loop {
            let event = with_timeout(
                Duration::from_millis(CLOCK_TIMEOUT_MS),
                clock.wait_for_event(clock_div),
            )
            .await;

            match event {
                Ok(ClockEvent::Tick) => {
                    if !clock_running_glob.get() {
                        clock_running_glob.set(true);
                    }

                    // Measure tick interval and update elapsed baseline.
                    let now = Instant::now();
                    if let Some(prev_tick) = last_tick {
                        let interval = now.duration_since(prev_tick).as_millis() as u32;
                        tick_interval_glob.set(interval);
                    }
                    last_tick = Some(now);

                    // Process any pending command from button_handler.
                    // PendingStart / PendingCommit are held until a 16th-note
                    // boundary (TICK_COUNTER % 6 == 0); all others fire immediately.
                    let cmd = cmd_glob.get();
                    if cmd != LooperCmd::None {
                        let on_16th = tick_fn().is_multiple_of(6);
                        let ready = match cmd {
                            LooperCmd::PendingStart | LooperCmd::PendingCommit => on_16th,
                            _ => true,
                        };

                        if ready {
                            cmd_glob.set(LooperCmd::None);
                            match cmd {
                                LooperCmd::PendingStart => {
                                    rec_head = 0;
                                    state_glob.set(LooperState::Recording);
                                }
                                LooperCmd::PendingCommit => {
                                    if rec_head > 0 {
                                        loop_len = rec_head;
                                        let rec_copy = rec_buf;
                                        storage.modify_and_save(|s| {
                                            let mut buf = [0u16; MAX_BUFFER_SAMPLES];
                                            buf[..loop_len].copy_from_slice(&rec_copy[..loop_len]);
                                            s.buffer.set(buf);
                                            s.has_loop = true;
                                            s.loop_len = loop_len as u16;
                                        });
                                        loop_start_tick = tick_fn() as u32;
                                        state_glob.set(LooperState::Playing);
                                        leds.set(0, Led::Button, led_color, Brightness::Mid);
                                    } else if state_glob.get() != LooperState::Playing {
                                        // Button released before any samples were captured — abort.
                                        state_glob.set(LooperState::Passthrough);
                                    }
                                    // else: auto-commit fired (buffer full) while button was held; stay Playing.
                                }
                                LooperCmd::ClearLoop => {
                                    loop_len = 1;
                                    rec_head = 0;
                                    storage.modify_and_save(|s| {
                                        s.has_loop = false;
                                        s.loop_len = 0;
                                    });
                                }
                                LooperCmd::LoadFromStorage => {
                                    // storage.inner was already updated by the scene recall;
                                    // just sync metadata and re-anchor the phase clock.
                                    loop_len = storage.query(|s| s.loop_len as usize).max(1);
                                    loop_start_tick = tick_fn() as u32;
                                }
                                LooperCmd::None => {}
                            }
                        }
                    }

                    let state = state_glob.get();
                    let fader_val = fader.get_value();
                    let att_val = att_glob.get();
                    let offset_val = offset_glob.get();

                    match state {
                        LooperState::Passthrough | LooperState::PendingRecording => {
                            // Output driven by main_loop (1 ms polling).
                        }
                        LooperState::Recording => {
                            if rec_head < MAX_BUFFER_SAMPLES {
                                rec_buf[rec_head] = fader_val;
                                rec_head += 1;
                            }
                            if rec_head >= MAX_BUFFER_SAMPLES {
                                // Buffer full — auto-commit and start looping immediately.
                                // rec_head is reset to 0 so a late PendingCommit (button still
                                // held) sees an empty count and stays in Playing rather than
                                // re-committing or aborting to Passthrough.
                                loop_len = MAX_BUFFER_SAMPLES;
                                let rec_copy = rec_buf;
                                storage.modify_and_save(|s| {
                                    s.buffer.set(rec_copy);
                                    s.has_loop = true;
                                    s.loop_len = MAX_BUFFER_SAMPLES as u16;
                                });
                                loop_start_tick = tick_fn() as u32;
                                rec_head = 0;
                                state_glob.set(LooperState::Playing);
                                leds.set(0, Led::Button, led_color, Brightness::Mid);
                            }
                            // CV output still driven by main_loop.
                        }
                        LooperState::Playing => {
                            let clkn = (tick_fn() as u32).wrapping_sub(loop_start_tick);
                            let read_head = (clkn / clock_div as u32) as usize % loop_len;
                            if read_head == 0 {
                                leds.set(0, Led::Button, Color::White, Brightness::High);
                            } else {
                                leds.set(0, Led::Button, led_color, Brightness::Mid);
                            }
                            let sample = storage.query(|s| s.buffer.at(read_head));
                            let attenuated = if bipolar {
                                attenuate_bipolar(sample, att_val)
                            } else {
                                attenuate(sample, att_val)
                            };
                            // Curved so the fader's center flat zone reliably
                            // lands on exactly zero offset instead of drifting near it.
                            let with_offset = (attenuated as i32
                                + Curve::Deadzone.at(offset_val) as i32
                                - 2048)
                                .clamp(0, 4095)
                                as u16;
                            // Roll interpolation window: current target → prev, new → target.
                            loop_prev_glob.set(loop_target_glob.get());
                            loop_target_glob.set(with_offset);
                            tick_id_glob.set(tick_id_glob.get().wrapping_add(1));
                        }
                    }
                }
                Ok(ClockEvent::Reset) => {
                    clock_running_glob.set(true);
                    loop_start_tick = 0;
                }
                Ok(ClockEvent::Start) => {
                    clock_running_glob.set(true);
                }
                Ok(ClockEvent::Stop) => {
                    clock_running_glob.set(false);
                }
                Err(_) => {
                    if clock_running_glob.get() {
                        clock_running_glob.set(false);
                    }
                }
            }
        }
    };

    // ── Button handler ───────────────────────────────────────────────────────
    // UI rules:
    //   Button (hold)   — record a new loop; start and end are quantised to the
    //                     next 16th-note boundary by the clock handler.
    //   Button (tap)    — while Playing: start overdub; while Overdubbing: end overdub.
    //   Shift + button  — clear loop and return to Passthrough (always works).
    let button_handler = async {
        loop {
            let (_, is_shift) = buttons.wait_for_any_down().await;
            let state = state_glob.get();

            if is_shift {
                // Shift+button clears the loop from any state.
                buttons.wait_for_any_up().await;
                state_glob.set(LooperState::Passthrough);
                cmd_glob.set(LooperCmd::ClearLoop);
                leds.set(0, Led::Button, led_color, Brightness::Mid);
                continue;
            }

            match state {
                LooperState::Passthrough => {
                    if !clock_running_glob.get() {
                        buttons.wait_for_any_up().await;
                        continue;
                    }
                    // Visual feedback starts immediately; actual recording begins
                    // on the next 16th-note boundary (processed by clock_handler).
                    state_glob.set(LooperState::PendingRecording);
                    cmd_glob.set(LooperCmd::PendingStart);
                    leds.set(0, Led::Button, Color::Red, Brightness::High);

                    buttons.wait_for_any_up().await;

                    // Queue commit; clock_handler decides Playing vs Passthrough
                    // based on whether any samples were captured (rec_head > 0).
                    // Restore button LED — clock_handler sets it to led_color on commit,
                    // or main_loop restores it if the abort path fires.
                    cmd_glob.set(LooperCmd::PendingCommit);
                }
                LooperState::PendingRecording | LooperState::Recording => {
                    // Should not be reachable (button is already held down).
                    buttons.wait_for_any_up().await;
                }
                LooperState::Playing => {
                    if !clock_running_glob.get() {
                        buttons.wait_for_any_up().await;
                        continue;
                    }
                    // Re-record over the existing loop, same flow as Passthrough.
                    state_glob.set(LooperState::PendingRecording);
                    cmd_glob.set(LooperCmd::PendingStart);
                    leds.set(0, Led::Button, Color::Red, Brightness::High);

                    buttons.wait_for_any_up().await;

                    cmd_glob.set(LooperCmd::PendingCommit);
                }
            }
        }
    };

    // ── Main loop ────────────────────────────────────────────────────────────
    // Runs every 1 ms.  Handles:
    //   • Fader latch (main layer = offset, alt layer via shift = attenuator).
    //   • Direct CV+MIDI output in Passthrough / PendingRecording / Recording.
    //   • Fader LEDs: signal level in led_color; when shift is held, attenuator
    //     level in red (mirrors control.rs behaviour).
    //   • Button LED updates when clock state changes in Passthrough.
    let main_loop = async {
        let mut latch = app.make_latch(fader.get_value());
        let mut last_midi_scaled: u32 = u32::MAX;
        let mut prev_state = state_glob.get();
        let mut prev_clock_running = clock_running_glob.get();
        let mut prev_slew_val: u16 = loop_target_glob.get();
        let mut elapsed_ms: u32 = 0;
        // Detect when clock_handler rolls the interpolation window forward.
        // Uses a counter rather than prev-sample value so detection is reliable
        // even when consecutive processed samples are equal.
        let mut last_tick_id: u8 = tick_id_glob.get();

        loop {
            app.delay_millis(1).await;

            // ── Fader latch ──────────────────────────────────────────────────
            // Latch always ticks so pickup state stays consistent, but results
            // are only applied when a loop is active.  During Passthrough /
            // PendingRecording / Recording the fader IS the signal — applying
            // an offset would corrupt the full range of a recorded gesture.
            let state = state_glob.get();
            let is_shift = buttons.is_shift_pressed();
            let latch_layer = LatchLayer::from(is_shift);
            let offset_target = storage.query(|s| s.offset_val);
            let att_target = storage.query(|s| s.att_val);
            let latch_target = match latch_layer {
                LatchLayer::Main => offset_target,
                LatchLayer::Alt => att_target,
                _ => 0,
            };

            let latch_active = state == LooperState::Playing;
            if let Some(new_val) = latch.update(fader.get_value(), latch_layer, latch_target) {
                if latch_active {
                    match latch_layer {
                        LatchLayer::Main => {
                            storage.modify(|s| s.offset_val = new_val);
                            offset_glob.set(new_val);
                        }
                        LatchLayer::Alt => {
                            storage.modify(|s| s.att_val = new_val);
                            att_glob.set(new_val);
                        }
                        _ => {}
                    }
                }
            }

            // Reset offset to center when a fresh loop commits so playback
            // always starts unmodified regardless of where the fader landed.
            let coming_from_record = matches!(
                prev_state,
                LooperState::Recording | LooperState::PendingRecording
            );
            if coming_from_record && state == LooperState::Playing {
                storage.modify(|s| s.offset_val = 2048);
                offset_glob.set(2048);
                // Seed the interpolation prev from current CV output so the transition
                // to the first loop sample is smooth instead of jumping from the stale
                // seed (0 or old loop start) that clock_handler left in loop_prev_glob.
                loop_prev_glob.set(prev_slew_val);
                latch = app.make_latch(fader.get_value());
            }

            // ── CV + MIDI output ─────────────────────────────────────────────
            // Compute the raw target for this state, send MIDI from raw fader,
            // then apply slew before driving the CV output.
            let raw_target = match state {
                LooperState::Passthrough
                | LooperState::PendingRecording
                | LooperState::Recording => {
                    let fader_val = fader.get_value();
                    let midi_val = midi_gate(fader_val, nrpn) as u32;
                    if last_midi_scaled != midi_val {
                        midi.send_cc(midi_cc, fader_val).await;
                        last_midi_scaled = midi_val;
                    }
                    fader_val
                }
                LooperState::Playing => {
                    // Reset elapsed when clock_handler rolls the window forward.
                    // Counter-based so detection fires even when consecutive samples are equal.
                    let current_tick_id = tick_id_glob.get();
                    if current_tick_id != last_tick_id {
                        elapsed_ms = 0;
                        last_tick_id = current_tick_id;
                    }
                    elapsed_ms += 1;
                    let interval = tick_interval_glob.get();
                    let interp = if interval > 0 {
                        interp_loop_sample(
                            loop_prev_glob.get(),
                            loop_target_glob.get(),
                            elapsed_ms,
                            interval,
                            clock_div as u8,
                        )
                    } else {
                        loop_target_glob.get()
                    };
                    let midi_val = midi_gate(interp, nrpn) as u32;
                    if last_midi_scaled != midi_val {
                        midi.send_cc(midi_cc, interp).await;
                        last_midi_scaled = midi_val;
                    }
                    interp
                }
            };
            prev_slew_val = match state {
                LooperState::Playing => raw_target,
                _ => slew_2(prev_slew_val, raw_target, 3, 10),
            };
            output.set_value(prev_slew_val);
            last_output_glob.set(prev_slew_val);

            // ── Fader LEDs ───────────────────────────────────────────────────
            // Shift held (alt layer): attenuator level in red.
            // Recording / PendingRecording: solid red.
            // Playing: solid green.
            // Passthrough: param color, level-modulated.
            let att_val = att_glob.get();
            match latch_layer {
                LatchLayer::Alt => {
                    let b = Brightness::Custom((att_val / 16) as u8);
                    leds.set(0, Led::Top, Color::Red, b);
                    if bipolar {
                        leds.set(0, Led::Bottom, Color::Red, b);
                    } else {
                        leds.unset(0, Led::Bottom);
                    }
                }
                _ => match state {
                    LooperState::Recording | LooperState::PendingRecording => {
                        let out = last_output_glob.get();
                        if bipolar {
                            let vals = split_unsigned_value(out);
                            leds.set(0, Led::Top, Color::Red, Brightness::Custom(vals[0]));
                            leds.set(0, Led::Bottom, Color::Red, Brightness::Custom(vals[1]));
                        } else {
                            leds.set(0, Led::Top, Color::Red, Brightness::Custom((out / 16) as u8));
                            leds.unset(0, Led::Bottom);
                        }
                    }
                    LooperState::Playing => {
                        let out = last_output_glob.get();
                        if bipolar {
                            let vals = split_unsigned_value(out);
                            leds.set(0, Led::Top, Color::Green, Brightness::Custom(vals[0]));
                            leds.set(0, Led::Bottom, Color::Green, Brightness::Custom(vals[1]));
                        } else {
                            leds.set(0, Led::Top, Color::Green, Brightness::Custom((out / 16) as u8));
                            leds.unset(0, Led::Bottom);
                        }
                    }
                    LooperState::Passthrough => {
                        let out = last_output_glob.get();
                        if bipolar {
                            let vals = split_unsigned_value(out);
                            leds.set(0, Led::Top, led_color, Brightness::Custom(vals[0]));
                            leds.set(0, Led::Bottom, led_color, Brightness::Custom(vals[1]));
                        } else {
                            leds.set(0, Led::Top, led_color, Brightness::Custom((out / 16) as u8));
                            leds.unset(0, Led::Bottom);
                        }
                    }
                },
            }

            // ── Button LED: restore led_color on state transitions ────────────
            let clock_running = clock_running_glob.get();
            if state != prev_state || clock_running != prev_clock_running {
                if prev_state == LooperState::Playing && state != LooperState::Playing {
                    last_midi_scaled = u32::MAX;
                }
                // Abort path (PendingCommit with rec_head == 0): restore button.
                // Recording: button set solid red by button_handler already.
                // Playing: button set to led_color by clock_handler on commit,
                //          or white flash on loop start — leave it alone.
                if state == LooperState::Passthrough {
                    leds.set(0, Led::Button, led_color, Brightness::Mid);
                }
                prev_state = state;
                prev_clock_running = clock_running;
            }
        }
    };

    let save_handler = async {
        loop {
            app.delay_secs(1).await;
            storage.save().await;
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    att_glob.set(storage.query(|s| s.att_val));
                    offset_glob.set(storage.query(|s| s.offset_val));

                    if storage.query(|s| s.has_loop) {
                        state_glob.set(LooperState::Playing);
                        cmd_glob.set(LooperCmd::LoadFromStorage);
                        leds.set(0, Led::Button, Color::Green, Brightness::High);
                    } else {
                        state_glob.set(LooperState::Passthrough);
                        leds.set(0, Led::Button, led_color, Brightness::Mid);
                    }
                }
                SceneEvent::SaveScene(scene) => {
                    storage.modify_and_save(|s| {
                        s.att_val = att_glob.get();
                        s.offset_val = offset_glob.get();
                    });
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    join5(
        clock_handler,
        button_handler,
        main_loop,
        save_handler,
        scene_handler,
    )
    .await;
}
