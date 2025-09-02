// todo :
// recall probability

use embassy_futures::{join::join5, select::select};

use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use libfp::{
    constants::BJORKLUND_PATTERNS, ext::FromValue, latch::LatchLayer, Brightness, Color, Config,
    Param, Value, APP_MAX_PARAMS,
};
use serde::{Deserialize, Serialize};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 5;

const LED_BRIGHTNESS: Brightness = Brightness::Lower;

pub static CONFIG: Config<PARAMS> = Config::new("Euclid", "Euclidean sequencer")
    .add_param(Param::i32 {
        name: "MIDI Channel",
        min: 1,
        max: 16,
    })
    .add_param(Param::i32 {
        name: "MIDI NOTE 1",
        min: 1,
        max: 128,
    })
    .add_param(Param::i32 {
        name: "MIDI NOTE 2",
        min: 1,
        max: 128,
    })
    .add_param(Param::i32 {
        name: "GATE %",
        min: 1,
        max: 100,
    })
    .add_param(Param::Color {
        name: "Color",
        variants: &[
            Color::Yellow,
            Color::Pink,
            Color::Blue,
            Color::Red,
            Color::White,
        ],
    });

pub struct Params {
    midi_channel: i32,
    note: i32,
    note2: i32,
    gatel: i32,
    color: Color,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_channel: 1,
            note: 32,
            note2: 33,
            gatel: 50,
            color: Color::Yellow,
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            midi_channel: i32::from_value(values[0]),
            note: i32::from_value(values[1]),
            note2: i32::from_value(values[2]),
            gatel: i32::from_value(values[3]),
            color: Color::from_value(values[4]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.note.into()).unwrap();
        vec.push(self.note2.into()).unwrap();
        vec.push(self.gatel.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    fader_saved: [u16; 2],
    shift_fader_saved: [u16; 2],
    div_saved: u16,
    mute_saved: bool,
    mode: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            fader_saved: [2000; 2],
            shift_fader_saved: [0, 4095],
            div_saved: 3000,
            mute_saved: false,
            mode: true,
        }
    }
}
impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(app.app_id, app.start_channel);

    let app_loop = async {
        loop {
            let storage = ManagedStorage::<Storage>::new(app.app_id, app.start_channel);
            param_store.load().await;
            storage.load(None).await;
            select(
                run(&app, &param_store, storage),
                param_store.param_handler(),
            )
            .await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(
    app: &App<CHANNELS>,
    params: &ParamStore<Params>,
    storage: ManagedStorage<Storage>,
) {
    let mut clock = app.use_clock();
    let die = app.use_die();
    let faders = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    let (midi_chan, note, note2, gatel, led_color) =
        params.query(|p| (p.midi_channel, p.note, p.note2, p.gatel, p.color));

    let midi = app.use_midi_output(midi_chan as u8 - 1);

    let glob_muted = app.make_global(false);
    let div_glob = app.make_global(6);
    let num_step_glob = app.make_global(16);
    let num_beat_glob = app.make_global(4);
    let rotation_glob = app.make_global(0);

    let glob_latch_layer = app.make_global(LatchLayer::Main);

    let jack = [
        app.make_gate_jack(0, 4095).await,
        app.make_gate_jack(1, 4095).await,
    ];

    let resolution = [384, 184, 92, 48, 24, 16, 12, 8, 6, 4, 3, 2];

    let mut clkn = 0;

    let (fader_saved, shift_fader_saved, mute) =
        storage.query(|s| (s.fader_saved, s.shift_fader_saved, s.mute_saved));

    num_beat_glob.set((fader_saved[0] as u32 * 15 / 4095) as u8 + 1);
    num_step_glob.set((fader_saved[1] as u32 * num_beat_glob.get() as u32 / 4095) as u8);

    rotation_glob.set((shift_fader_saved[0] as u32 * num_beat_glob.get() as u32 / 4095) as u8);

    glob_muted.set(mute);

    if mute {
        leds.unset(1, Led::Button);
    } else {
        leds.set(1, Led::Button, led_color, LED_BRIGHTNESS);
    }
    leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);

    let fut1 = async {
        let mut note_on = false;
        let mut aux_on = false;

        loop {
            match clock.wait_for_event(1).await {
                ClockEvent::Reset => {
                    clkn = 0;
                    midi.send_note_off(note as u8 - 1).await;
                    midi.send_note_off(note2 as u8 - 1).await;
                    note_on = false;
                    jack[0].set_low().await;
                }
                ClockEvent::Tick => {
                    let muted = glob_muted.get();
                    let div = div_glob.get();

                    if clkn % div == 0 {
                        if !muted {
                            if euclidean_filter(
                                num_beat_glob.get(),
                                num_step_glob.get(),
                                rotation_glob.get(),
                                (clkn / div) as u32,
                            ) && storage.query(|s| (s.shift_fader_saved[1]))
                                >= die.roll().clamp(100, 3900)
                            {
                                midi.send_note_on(note as u8 - 1, 4095).await;
                                // info!("proba {}", storage.query(|s| (s.shift_fader_saved[1])));
                                jack[0].set_high().await;
                                note_on = true;
                            }
                            if storage.query(|s| s.mode) {
                                if clkn / div % num_beat_glob.get() as u16 == 0 {
                                    jack[1].set_high().await;
                                    midi.send_note_on(note2 as u8 - 1, 4095).await;

                                    aux_on = true;
                                }
                            } else if !note_on {
                                jack[1].set_high().await;
                                midi.send_note_on(note2 as u8 - 1, 4095).await;
                                aux_on = true;
                            }
                        }

                        if glob_latch_layer.get() == LatchLayer::Third {
                            leds.set(0, Led::Bottom, Color::Red, Brightness::Lower);
                        }
                    }

                    if clkn % div == (div * gatel as u16 / 100).clamp(1, div - 1) {
                        if note_on {
                            midi.send_note_off(note as u8 - 1).await;

                            note_on = false;
                            jack[0].set_low().await;
                        }
                        if aux_on {
                            midi.send_note_off(note2 as u8 - 1).await;
                            aux_on = false;
                            jack[1].set_low().await;
                        }

                        leds.set(0, Led::Bottom, led_color, Brightness::Custom(0))
                    }
                    if glob_latch_layer.get() == LatchLayer::Main {
                        if note_on {
                            leds.set(0, Led::Top, led_color, Brightness::Default);
                        } else {
                            leds.set(0, Led::Top, led_color, Brightness::Custom(0))
                        }
                        if aux_on {
                            leds.set(1, Led::Top, led_color, Brightness::Default);
                        } else {
                            leds.set(1, Led::Top, led_color, Brightness::Custom(0))
                        }
                    }
                    if glob_latch_layer.get() == LatchLayer::Alt {
                        leds.set(
                            0,
                            Led::Top,
                            Color::Red,
                            Brightness::Custom(
                                (storage.query(|s| (s.shift_fader_saved[0])) / 16) as u8,
                            ),
                        );
                        leds.set(
                            1,
                            Led::Top,
                            Color::Red,
                            Brightness::Custom(
                                (storage.query(|s| (s.shift_fader_saved[1])) / 16) as u8,
                            ),
                        );
                    }
                    clkn += 1;
                }
                _ => {}
            }
        }
    };

    let fut2 = async {
        loop {
            let (chan, shift) = buttons.wait_for_any_down().await;
            if !shift {
                if chan == 1 {
                    let muted = glob_muted.toggle();

                    storage
                        .modify_and_save(
                            |s| {
                                s.mute_saved = muted;
                                s.mute_saved
                            },
                            None,
                        )
                        .await;

                    if muted {
                        jack[0].set_low().await;
                        leds.unset_chan(1);
                    } else {
                        leds.set(1, Led::Button, led_color, LED_BRIGHTNESS);
                    }
                }
            } else if chan == 1 {
                let mut mode = storage.query(|s| s.mode);
                mode = !mode;
                storage
                    .modify_and_save(
                        |s| {
                            s.mode = mode;
                            s.mode
                        },
                        None,
                    )
                    .await;
                if !mode {
                    leds.set(1, Led::Button, led_color, Brightness::Low);
                } else {
                    leds.set(1, Led::Button, led_color, Brightness::Lowest);
                }
            }
        }
    };

    let fut3 = async {
        let mut latch = [
            app.make_latch(faders.get_value_at(0)),
            app.make_latch(faders.get_value_at(1)),
        ];
        loop {
            let chan = faders.wait_for_any_change().await;
            let latch_layer = glob_latch_layer.get();
            if chan == 0 {
                let target_value = match latch_layer {
                    LatchLayer::Main => storage.query(|s| s.fader_saved[chan]),
                    LatchLayer::Alt => storage.query(|s| s.shift_fader_saved[chan]),
                    LatchLayer::Third => storage.query(|s| s.div_saved),
                    _ => unreachable!(),
                };
                if let Some(new_value) =
                    latch[chan].update(faders.get_value_at(chan), latch_layer, target_value)
                {
                    match latch_layer {
                        LatchLayer::Main => {
                            num_beat_glob.set((new_value as u32 * 15 / 4095) as u8 + 1);
                            num_step_glob.set(storage.query(|s| {
                                s.fader_saved[1] as u32 * num_beat_glob.get() as u32 / 4095
                            }) as u8);
                            let shift_stored_faders = storage.query(|s| s.shift_fader_saved);

                            rotation_glob.set(
                                (shift_stored_faders[0] as u32 * num_beat_glob.get() as u32 / 4095)
                                    as u8,
                            );

                            storage
                                .modify_and_save(
                                    |s| {
                                        s.fader_saved[chan] = new_value;
                                    },
                                    None,
                                )
                                .await;
                        }
                        LatchLayer::Alt => {
                            rotation_glob
                                .set((new_value as u32 * num_beat_glob.get() as u32 / 4095) as u8);
                            storage
                                .modify_and_save(
                                    |s| {
                                        s.shift_fader_saved[chan] = new_value;
                                    },
                                    None,
                                )
                                .await;
                        }
                        LatchLayer::Third => {
                            div_glob.set(resolution[new_value as usize / 345]);
                            storage
                                .modify_and_save(|s| s.div_saved = new_value, None)
                                .await;
                        }
                        _ => unreachable!(),
                    }
                }
            } else {
                let target_value = match latch_layer {
                    LatchLayer::Main => storage.query(|s| s.fader_saved[chan]),
                    LatchLayer::Alt => storage.query(|s| s.shift_fader_saved[chan]),
                    LatchLayer::Third => 0,
                    _ => unreachable!(),
                };
                if let Some(new_value) =
                    latch[chan].update(faders.get_value_at(chan), latch_layer, target_value)
                {
                    match latch_layer {
                        LatchLayer::Main => {
                            num_step_glob
                                .set((new_value as u32 * num_beat_glob.get() as u32 / 4095) as u8);

                            storage
                                .modify_and_save(
                                    |s| {
                                        s.fader_saved[chan] = new_value;
                                    },
                                    None,
                                )
                                .await;
                        }
                        LatchLayer::Alt => {
                            storage
                                .modify_and_save(
                                    |s| {
                                        s.shift_fader_saved[chan] = new_value;
                                    },
                                    None,
                                )
                                .await;
                        }
                        LatchLayer::Third => {}
                        _ => unreachable!(),
                    }
                }
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load(Some(scene)).await;

                    num_beat_glob
                        .set((storage.query(|s| s.fader_saved[0]) as u32 * 15 / 4095) as u8 + 1);
                    num_step_glob.set(
                        (storage.query(|s| s.fader_saved[1]) as u32 * num_beat_glob.get() as u32
                            / 4095) as u8,
                    );

                    rotation_glob.set(
                        (storage.query(|s| s.shift_fader_saved[0]) as u32
                            * num_beat_glob.get() as u32
                            / 4095) as u8,
                    );
                    glob_muted.set(storage.query(|s| s.mute_saved));

                    let division = storage.query(|s| s.div_saved);
                    div_glob.set(resolution[division as usize / 345]);
                }

                SceneEvent::SaveScene(scene) => {
                    storage.save(Some(scene)).await;
                }
            }
        }
    };

    let shift = async {
        loop {
            // latching on pressing and depressing shift

            app.delay_millis(1).await;

            let latch_active_layer = if buttons.is_shift_pressed() && !buttons.is_button_pressed(0)
            {
                LatchLayer::Alt
            } else if !buttons.is_shift_pressed() && buttons.is_button_pressed(0) {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            glob_latch_layer.set(latch_active_layer);

            if latch_active_layer == LatchLayer::Main {
                if glob_muted.get() {
                    leds.unset(1, Led::Button);
                } else {
                    leds.set(1, Led::Button, led_color, Brightness::Lower);
                }
            }
            if latch_active_layer == LatchLayer::Alt {
                if !storage.query(|s| s.mode) {
                    leds.set(1, Led::Button, led_color, Brightness::Low);
                } else {
                    leds.set(1, Led::Button, led_color, Brightness::Lowest);
                }
            }
            if latch_active_layer == LatchLayer::Third {}
        }
    };

    join5(fut1, fut2, fut3, scene_handler, shift).await;
}

/// Rotate left a u32 pattern within a given bit width
fn rotl32(value: u32, width: u8, rotation: u8) -> u32 {
    let rotation = rotation % width;
    ((value << rotation) | (value >> (width - rotation))) & ((1 << width) - 1)
}

/// Get the Euclidean pattern as a u32
fn euclidean_pattern(num_steps: u8, num_beats: u8, rotation: u8, padding: u8) -> u32 {
    let steps = num_steps.max(2);
    let beats = num_beats.min(steps);
    let index = ((steps - 2) as usize) * 33 + beats as usize;

    let mut pattern = BJORKLUND_PATTERNS.get(index).copied().unwrap_or(0);

    if rotation > 0 {
        let rot = rotation % (steps + padding);
        pattern = rotl32(pattern, steps + padding, rot);
    }

    pattern
}

/// Check if there's a beat at a given clock position
fn euclidean_filter(num_steps: u8, num_beats: u8, rotation: u8, clock: u32) -> bool {
    let pattern = euclidean_pattern(num_steps, num_beats, rotation, 0);
    let pos = (clock % num_steps as u32) as u8;
    (pattern & (1 << pos)) != 0
}
