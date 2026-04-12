use embassy_futures::{
    join::join5,
    select::{select, select3, Either},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use embassy_time::{with_timeout, Duration};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    utils::slew_2,
    AppIcon, Brightness, ClockDivision, Color, Config, MidiCc, MidiChannel, MidiOut, Param, Range,
    Value, APP_MAX_PARAMS,
};

use crate::{
    app::{
        App, AppParams, AppStorage, Arr, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
    },
    tasks::leds::LedMode,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 6;

const MAX_BUFFER_SAMPLES: usize = 192;
const SAMPLES_PER_BAR: usize = 48;
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
.add_param(Param::MidiCc {
    name: "MIDI CC",
})
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
.add_param(Param::MidiOut);

pub struct Params {
    midi_channel: MidiChannel,
    midi_cc: MidiCc,
    range: Range,
    color: Color,
    nrpn: bool,
    midi_out: MidiOut,
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
        vec
    }
}

#[derive(Clone, Copy, PartialEq)]
enum LooperState {
    Passthrough,
    Playing,
    Overdubbing,
}

/// Command issued by the button handler, processed by the clock handler.
/// Using a command enum avoids race conditions — the clock handler performs
/// all buffer mutations at the sample tick boundary.
#[derive(Clone, Copy, PartialEq)]
enum LooperCmd {
    None,
    /// Passthrough → Playing: commit the loop from the circular buffer
    CommitLoop,
    /// Playing → Overdubbing: snapshot buffer into undo slot, begin overdub
    BeginOverdub,
    /// Overdubbing → Playing: stop writing, resume playback
    EndOverdub,
    /// Overdubbing + Hold: restore undo buffer, resume playback
    UndoOverdub,
    /// Playing + Hold: return to passthrough
    StopPlayback,
    /// Scene load: reload play buffer from storage
    LoadFromStorage,
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    loop_bars: u8,
    /// The committed playback loop (not the circular write buffer).
    buffer: Arr<u16, MAX_BUFFER_SAMPLES>,
    has_loop: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            loop_bars: 2,
            buffer: Arr::default(),
            has_loop: false,
        }
    }
}

impl AppStorage for Storage {}

fn loop_len_samples(bars: u8) -> usize {
    bars as usize * SAMPLES_PER_BAR
}

fn next_loop_bars(bars: u8) -> u8 {
    match bars {
        1 => 2,
        2 => 4,
        _ => 1,
    }
}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            midi_channel: MidiChannel::default(),
            midi_cc: MidiCc::default(),
            range: Range::_0_10V,
            color: Color::Cyan,
            nrpn: false,
            midi_out: MidiOut([false, false, false]),
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
    let (midi_out, midi_chan, midi_cc, range, nrpn, led_color) =
        params.query(|p| (p.midi_out, p.midi_channel, p.midi_cc, p.range, p.nrpn, p.color));

    let mut clock = app.use_clock();
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let midi = app.use_midi_output(midi_out, midi_chan, nrpn);
    let output = app.make_out_jack(0, range).await;

    let state_glob = app.make_global(LooperState::Passthrough);
    let cmd_glob = app.make_global(LooperCmd::None);
    let clock_running_glob = app.make_global(false);
    let loop_bars_glob = app.make_global(storage.query(|s| s.loop_bars));

    // Restore saved loop if available
    let has_saved_loop = storage.query(|s| s.has_loop);
    if has_saved_loop {
        state_glob.set(LooperState::Playing);
    }

    let clock_handler = async {
        // Circular write buffer (ephemeral, used in Passthrough)
        let mut write_buf = [0u16; MAX_BUFFER_SAMPLES];
        let mut write_head: usize = 0;

        // Playback buffer (the committed loop)
        let mut play_buf = [0u16; MAX_BUFFER_SAMPLES];
        let mut undo_buf = [0u16; MAX_BUFFER_SAMPLES];
        let mut has_undo = false;
        let mut read_head: usize = 0;
        let mut loop_len = loop_len_samples(loop_bars_glob.get());

        // Restore from storage if we have a saved loop
        if has_saved_loop {
            play_buf = storage.query(|s| s.buffer.get());
        }

        let mut prev_slew_val: u16 = 0;

        loop {
            let event = with_timeout(
                Duration::from_millis(CLOCK_TIMEOUT_MS),
                clock.wait_for_event(ClockDivision::_2),
            )
            .await;

            match event {
                Ok(ClockEvent::Tick) => {
                    if !clock_running_glob.get() {
                        clock_running_glob.set(true);
                    }

                    // Process any pending command from button handler
                    let cmd = cmd_glob.get();
                    if cmd != LooperCmd::None {
                        cmd_glob.set(LooperCmd::None);

                        match cmd {
                            LooperCmd::CommitLoop => {
                                // Copy the last loop_len samples from the circular write buffer
                                // into the playback buffer
                                loop_len = loop_len_samples(loop_bars_glob.get());
                                let start = (write_head + MAX_BUFFER_SAMPLES - loop_len)
                                    % MAX_BUFFER_SAMPLES;
                                for i in 0..loop_len {
                                    play_buf[i] = write_buf[(start + i) % MAX_BUFFER_SAMPLES];
                                }
                                read_head = 0;
                                has_undo = false;
                                // Persist committed loop
                                let buf_copy = play_buf;
                                storage.modify_and_save(|s| {
                                    s.buffer.set(buf_copy);
                                    s.has_loop = true;
                                });
                            }
                            LooperCmd::BeginOverdub => {
                                undo_buf = play_buf;
                                has_undo = true;
                            }
                            LooperCmd::EndOverdub => {
                                // Persist the overdubbed loop
                                let buf_copy = play_buf;
                                storage.modify_and_save(|s| {
                                    s.buffer.set(buf_copy);
                                });
                            }
                            LooperCmd::UndoOverdub => {
                                if has_undo {
                                    play_buf = undo_buf;
                                    has_undo = false;
                                }
                            }
                            LooperCmd::StopPlayback => {
                                // write_head continues from where it was frozen
                            }
                            LooperCmd::LoadFromStorage => {
                                play_buf = storage.query(|s| s.buffer.get());
                                loop_len = loop_len_samples(loop_bars_glob.get());
                                read_head = 0;
                                has_undo = false;
                            }
                            LooperCmd::None => {}
                        }
                    }

                    let state = state_glob.get();
                    let fader_val = fader.get_value();

                    match state {
                        LooperState::Passthrough => {
                            write_buf[write_head] = fader_val;
                            write_head = (write_head + 1) % MAX_BUFFER_SAMPLES;
                            output.set_value(fader_val);
                            midi.send_cc(midi_cc, fader_val).await;
                        }
                        LooperState::Playing => {
                            let sample = play_buf[read_head % loop_len];
                            prev_slew_val = slew_2(prev_slew_val, sample, 3, 10);
                            output.set_value(prev_slew_val);
                            midi.send_cc(midi_cc, prev_slew_val).await;
                            read_head = (read_head + 1) % loop_len;
                        }
                        LooperState::Overdubbing => {
                            let idx = read_head % loop_len;
                            play_buf[idx] = fader_val;
                            output.set_value(fader_val);
                            midi.send_cc(midi_cc, fader_val).await;
                            read_head = (read_head + 1) % loop_len;
                        }
                    }
                }
                Ok(ClockEvent::Reset) => {
                    // MIDI Start — re-align read head to bar start
                    clock_running_glob.set(true);
                    read_head = 0;
                }
                Ok(ClockEvent::Start) => {
                    // MIDI Continue — resume from frozen position
                    clock_running_glob.set(true);
                }
                Ok(ClockEvent::Stop) => {
                    clock_running_glob.set(false);
                }
                Err(_) => {
                    // Clock dropout — treated as stop (resume = Continue semantics)
                    if clock_running_glob.get() {
                        clock_running_glob.set(false);
                    }
                }
            }
        }
    };

    let button_handler = async {
        loop {
            buttons.wait_for_any_down().await;

            if !clock_running_glob.get() {
                // All button input is ignored when clock is stopped
                continue;
            }

            // Discriminate tap vs hold: race long_press against button up
            match select(
                buttons.wait_for_any_long_press(),
                buttons.wait_for_any_up(),
            )
            .await
            {
                Either::First(_) => {
                    // Hold
                    let state = state_glob.get();
                    match state {
                        LooperState::Passthrough => {
                            // Cycle loop length: 1 → 2 → 4 → 1
                            let bars = next_loop_bars(loop_bars_glob.get());
                            loop_bars_glob.set(bars);
                            storage.modify_and_save(|s| s.loop_bars = bars);
                            // Flash white N times to confirm
                            leds.set_mode(
                                0,
                                Led::Button,
                                LedMode::Flash(Color::White, Some(bars as usize)),
                            );
                        }
                        LooperState::Playing => {
                            // Stop playback → Passthrough
                            state_glob.set(LooperState::Passthrough);
                            cmd_glob.set(LooperCmd::StopPlayback);
                            // Yellow flash then off
                            leds.set_mode(0, Led::Button, LedMode::FadeOut(Color::Yellow));
                        }
                        LooperState::Overdubbing => {
                            // Undo overdub → Playing
                            state_glob.set(LooperState::Playing);
                            cmd_glob.set(LooperCmd::UndoOverdub);
                            // Yellow flash then green
                            leds.set_mode(
                                0,
                                Led::Button,
                                LedMode::FlashThenStatic(
                                    Color::Yellow,
                                    1,
                                    Color::Green,
                                    Brightness::High,
                                ),
                            );
                        }
                    }
                }
                Either::Second(_) => {
                    // Tap (released before long press threshold)
                    let state = state_glob.get();
                    match state {
                        LooperState::Passthrough => {
                            // Commit loop → Playing
                            state_glob.set(LooperState::Playing);
                            cmd_glob.set(LooperCmd::CommitLoop);
                            leds.set(0, Led::Button, Color::Green, Brightness::High);
                        }
                        LooperState::Playing => {
                            // Begin overdub
                            state_glob.set(LooperState::Overdubbing);
                            cmd_glob.set(LooperCmd::BeginOverdub);
                            leds.set(0, Led::Button, Color::Red, Brightness::High);
                        }
                        LooperState::Overdubbing => {
                            // End overdub → Playing
                            state_glob.set(LooperState::Playing);
                            cmd_glob.set(LooperCmd::EndOverdub);
                            leds.set(0, Led::Button, Color::Green, Brightness::High);
                        }
                    }
                }
            }
        }
    };

    let led_handler = async {
        let mut prev_clock_running = false;
        let mut prev_state = LooperState::Passthrough;

        // Initial LED state
        if has_saved_loop {
            leds.set(0, Led::Button, Color::Green, Brightness::High);
        } else {
            leds.set_mode(0, Led::Button, LedMode::Flash(Color::White, None));
        }

        loop {
            app.delay_millis(50).await;
            let state = state_glob.get();
            let clock_running = clock_running_glob.get();

            // Only update LEDs on state/clock changes to avoid overriding
            // transition flashes set by the button handler
            if state != prev_state || clock_running != prev_clock_running {
                match state {
                    LooperState::Passthrough => {
                        if clock_running {
                            leds.set(0, Led::Button, led_color, Brightness::Mid);
                        } else {
                            leds.set_mode(
                                0,
                                Led::Button,
                                LedMode::Flash(Color::White, None),
                            );
                        }
                    }
                    LooperState::Playing => {
                        // Don't override transition flash — button handler already set the LED
                        if prev_state == state && !clock_running {
                            // Clock stopped during playback — keep green but frozen
                            leds.set(0, Led::Button, Color::Green, Brightness::High);
                        }
                    }
                    LooperState::Overdubbing => {
                        if prev_state == state && !clock_running {
                            leds.set(0, Led::Button, Color::Red, Brightness::High);
                        }
                    }
                }
                prev_state = state;
                prev_clock_running = clock_running;
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    loop_bars_glob.set(storage.query(|s| s.loop_bars));

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
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    // Ensures responsive CV output between clock ticks and when clock is stopped.
    let passthrough_loop = async {
        loop {
            app.delay_millis(1).await;
            let state = state_glob.get();
            match state {
                LooperState::Passthrough => {
                    output.set_value(fader.get_value());
                }
                LooperState::Overdubbing => {
                    output.set_value(fader.get_value());
                }
                LooperState::Playing => {}
            }
        }
    };

    join5(
        clock_handler,
        button_handler,
        led_handler,
        scene_handler,
        passthrough_loop,
    )
    .await;
}
