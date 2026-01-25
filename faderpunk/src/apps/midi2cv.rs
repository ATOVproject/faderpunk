use embassy_futures::{
    join::join5,
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;

use libm::expf;
use midly::{num::u7, MidiMessage};
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{bits_7_16, clickless, scale_bits_7_12},
    AppIcon, Brightness, Color, Config, Curve, MidiCc, MidiChannel, MidiIn, MidiNote, Param, Range,
    Value, APP_MAX_PARAMS,
};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 9;

const LED_BRIGHTNESS: Brightness = Brightness::Mid;

pub static CONFIG: Config<PARAMS> = Config::new(
    "MIDI to CV",
    "Multifunctional MIDI to CV",
    Color::Cyan,
    AppIcon::KnobRound,
)
.add_param(Param::Enum {
    name: "Mode",
    variants: &["CC", "Pitch", "Gate", "Velocity", "AT", "Bend", "Note Gate"],
})
.add_param(Param::Curve {
    name: "Curve",
    variants: &[Curve::Linear, Curve::Logarithmic, Curve::Exponential],
})
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiCc { name: "MIDI CC" })
.add_param(Param::i32 {
    name: "Bend Range",
    min: 1,
    max: 24,
})
.add_param(Param::MidiNote { name: "MIDI Note" })
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
.add_param(Param::MidiIn)
.add_param(Param::bool {
    name: "Velocity on Gate",
});

pub struct Params {
    mode: usize,
    curve: Curve,
    midi_channel: MidiChannel,
    midi_cc: MidiCc,
    midi_note: MidiNote,
    midi_in: MidiIn,
    bend_range: i32,
    color: Color,
    gate_vel: bool,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            mode: 0,
            curve: Curve::Linear,
            midi_channel: MidiChannel::default(),
            midi_cc: MidiCc::from(32),
            midi_note: MidiNote::from(36),
            midi_in: MidiIn::default(),
            bend_range: 12,
            color: Color::Cyan,
            gate_vel: false,
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            mode: usize::from_value(values[0]),
            curve: Curve::from_value(values[1]),
            midi_channel: MidiChannel::from_value(values[2]),
            midi_cc: MidiCc::from_value(values[3]),
            bend_range: i32::from_value(values[4]),
            midi_note: MidiNote::from_value(values[5]),
            color: Color::from_value(values[6]),
            midi_in: MidiIn::from_value(values[7]),
            gate_vel: bool::from_value(values[8]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.mode.into()).unwrap();
        vec.push(self.curve.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(self.bend_range.into()).unwrap();
        vec.push(self.midi_note.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.midi_in.into()).unwrap();
        vec.push(self.gate_vel.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    muted: bool,
    alt_layer_val: u16,
    main_layer_val: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            muted: false,
            alt_layer_val: 4095,
            main_layer_val: 2048,
        }
    }
}

impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(app.app_id, app.layout_id);
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

/// RC filter glide calculation.
/// Returns the coefficient for exponential approach based on glide time.
/// At glide=0, returns 1.0 (instant). At glide=100, returns a small value for slow glide.
fn calc_glide_coeff(glide: i32) -> f32 {
    if glide == 0 {
        1.0
    } else {
        // RC time constant: larger glide value = slower approach
        // With 1ms tick rate, we need coefficients that give ~150ms settling
        // coeff = 1 - e^(-1/tau) where tau is in ticks
        let tau = 1.0 + (glide as f32 * 0.5);
        1.0 - expf(-1.0 / tau)
    }
}

/// Apply RC filter glide: moves current toward target exponentially
fn apply_glide(current: f32, target: f32, coeff: f32) -> f32 {
    current + (target - current) * coeff
}

pub async fn run(
    app: &App<CHANNELS>,
    params: &ParamStore<Params>,
    storage: &ManagedStorage<Storage>,
) {
    let (midi_in, midi_chan, midi_cc, curve, bend_range, led_color, note, mode, gate_vel) = params
        .query(|p| {
            (
                p.midi_in,
                p.midi_channel,
                p.midi_cc,
                p.curve,
                p.bend_range,
                p.color,
                p.midi_note,
                p.mode,
                p.gate_vel,
            )
        });

    let mut midi_in = app.use_midi_input(midi_in, midi_chan);
    let muted_glob = app.make_global(false);

    let offset_glob = app.make_global(0);
    let pitch_glob = app.make_global(0);
    let glide_active_glob = app.make_global(false);
    let glide_coeff_glob = app.make_global(calc_glide_coeff(
        storage.query(|s| s.alt_layer_val) as i32 * 100 / 4095,
    ));
    let buttons = app.use_buttons();
    let fader = app.use_faders();
    let leds = app.use_leds();

    let glob_latch_layer = app.make_global(LatchLayer::Main);

    let muted = storage.query(|s| s.muted);
    muted_glob.set(muted);

    if muted {
        leds.unset(0, Led::Button);
    } else {
        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
    }

    let jack = if mode != 5 {
        app.make_out_jack(0, Range::_0_10V).await
    } else {
        app.make_out_jack(0, Range::_Neg5_5V).await
    };

    if mode == 5 || mode == 1 {
        jack.set_value(2048);
        offset_glob.set(2048);
    } else {
        jack.set_value(0);
    }

    let handle_note_off = |key: u7, note_num: &mut i32| {
        // Handle note-off for pitch mode (mode 1)
        if mode == 1 {
            *note_num = (*note_num - 1).max(0);
            if *note_num == 0 {
                glide_active_glob.set(false);
            }
        } else if mode == 2 || (mode == 6 && key == u7::from(note)) {
            *note_num = (*note_num - 1).max(0);
            if *note_num == 0 {
                jack.set_value(0);
                leds.unset(0, Led::Top);
            }
        }
    };

    let output_handler = async {
        let mut outval = 0;
        let mut val;
        let mut attval;
        let mut fadval = fader.get_value();
        let mut glide_current: f32 = 0.0;

        loop {
            app.delay_millis(1).await;
            let latch_active_layer =
                glob_latch_layer.set(LatchLayer::from(buttons.is_shift_pressed()));

            match mode {
                0 | 4 => {
                    let muted = muted_glob.get();
                    if !buttons.is_shift_pressed() {
                        fadval = fader.get_value();
                    }
                    let att = storage.query(|s| s.alt_layer_val);
                    let offset = offset_glob.get();

                    if muted {
                        val = 0;
                    } else {
                        val = curve.at(fadval + offset);
                    }

                    outval = clickless(outval, val);
                    attval = ((outval as u32 * att as u32) / 4095) as u16;

                    jack.set_value(attval);
                    if latch_active_layer == LatchLayer::Alt {
                        leds.set(
                            0,
                            Led::Top,
                            Color::Red,
                            Brightness::Custom((att / 16) as u8),
                        );
                        leds.unset(0, Led::Bottom);
                    } else {
                        leds.set(
                            0,
                            Led::Top,
                            led_color,
                            Brightness::Custom((attval as f32 / 16.0) as u8),
                        );
                    }
                }
                1 => {
                    let offset = if !muted_glob.get() {
                        offset_glob.get()
                    } else {
                        2047
                    };

                    let pitch_target = pitch_glob.get() as f32;

                    // Only glide when legato (glide_active is true)
                    let glide_coeff = glide_coeff_glob.get();

                    if glide_active_glob.get() {
                        glide_current = apply_glide(glide_current, pitch_target, glide_coeff);
                    } else {
                        glide_current = pitch_target;
                    }
                    let pitch = glide_current as u16;

                    outval = clickless(outval, offset);
                    let out = (pitch as i32 + outval as i32 - 2047).clamp(0, 4095) as u16;
                    jack.set_value(out);

                    if latch_active_layer == LatchLayer::Alt {
                        let glide_amount = storage.query(|s| s.alt_layer_val);
                        leds.set(
                            0,
                            Led::Top,
                            Color::Red,
                            Brightness::Custom((glide_amount / 16) as u8),
                        );
                    } else {
                        leds.set(
                            0,
                            Led::Top,
                            led_color,
                            Brightness::Custom((pitch / 16) as u8),
                        );
                    }
                }
                5 => {
                    if !muted_glob.get() {
                        let offset = offset_glob.get();
                        outval = clickless(outval, offset);
                        jack.set_value(outval);
                    } else {
                        let offset = 2048;
                        outval = clickless(outval, offset);
                        jack.set_value(outval);
                    }
                }
                _ => {}
            }
        }
    };

    let button_handler = async {
        loop {
            buttons.wait_for_down(0).await;

            let muted = storage.modify_and_save(|s| {
                s.muted = !s.muted;
                s.muted
            });
            muted_glob.set(muted);
            if muted {
                leds.unset(0, Led::Button);
            } else {
                leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
            }
            if mode == 3 {
                jack.set_value(0);
                leds.unset(0, Led::Top);
            }
        }
    };

    let fader_handler = async {
        let mut latch = app.make_latch(fader.get_value());

        loop {
            fader.wait_for_change().await;

            let latch_layer = glob_latch_layer.get();
            let main_layer_value = storage.query(|s| s.main_layer_val);
            let alt_layer_value = storage.query(|s| s.alt_layer_val);

            let target_value = match latch_layer {
                LatchLayer::Main => main_layer_value,
                LatchLayer::Alt => alt_layer_value,
                LatchLayer::Third => 0,
            };

            if let Some(new_value) = latch.update(fader.get_value(), latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Main => {
                        storage.modify_and_save(|s| s.main_layer_val = new_value);
                    }
                    LatchLayer::Alt => {
                        storage.modify_and_save(|s| s.alt_layer_val = new_value);
                        if mode == 1 {
                            let glide = new_value as i32 * 100 / 4095;
                            glide_coeff_glob.set(calc_glide_coeff(glide));
                        }
                    }
                    LatchLayer::Third => {}
                }
            }
        }
    };

    let midi_handler = async {
        let mut note_num: i32 = 0;
        loop {
            match midi_in.wait_for_message().await {
                MidiMessage::Controller { controller, value } => {
                    if mode == 0 && controller == u7::from(midi_cc) {
                        let val = scale_bits_7_12(value);
                        offset_glob.set(val);
                    }
                }
                MidiMessage::NoteOn { key, vel } => {
                    // Sometimes note-off will be a NoteOn with velocity 0
                    if vel == 0 {
                        handle_note_off(key, &mut note_num);
                    } else {
                        match mode {
                            1 => {
                                if !muted_glob.get() {
                                    // Legato detection: if a note is already held, enable glide
                                    let is_legato = note_num > 0;
                                    glide_active_glob.set(is_legato);
                                    note_num += 1;

                                    let mut note_in = bits_7_16(key);
                                    note_in = (note_in as u32 * 410 / 12) as u16;
                                    let main_val = storage.query(|s| s.main_layer_val);
                                    let oct = (main_val as i32 * 10 / 4095) - 5;
                                    let note_out =
                                        (note_in as i32 + oct * 410).clamp(0, 4095) as u16;
                                    pitch_glob.set(note_out);
                                    leds.set(
                                        0,
                                        Led::Top,
                                        led_color,
                                        Brightness::Custom((note_out / 16) as u8),
                                    );
                                }
                            }
                            2 => {
                                if !muted_glob.get() {
                                    let vel_out = if gate_vel {
                                        (scale_bits_7_12(vel) as u32 * 3685 / 4095 + 410) as u16
                                    } else {
                                        4095
                                    };
                                    jack.set_value(vel_out);
                                    note_num += 1;
                                    leds.set(0, Led::Top, led_color, LED_BRIGHTNESS);
                                } else {
                                    note_num = 0;
                                }
                            }
                            3 => {
                                let vel_out = if !muted_glob.get() {
                                    scale_bits_7_12(vel)
                                } else {
                                    0
                                };
                                jack.set_value(vel_out);

                                leds.set(
                                    0,
                                    Led::Top,
                                    led_color,
                                    Brightness::Custom((vel_out / 16) as u8),
                                );
                            }
                            6 => {
                                if key == u7::from(note) {
                                    if !muted_glob.get() {
                                        let vel_out = if gate_vel {
                                            (scale_bits_7_12(vel) as u32 * 3685 / 4095 + 410) as u16
                                        } else {
                                            4095
                                        };
                                        jack.set_value(vel_out);
                                        note_num += 1;
                                        leds.set(0, Led::Top, led_color, LED_BRIGHTNESS);
                                    } else {
                                        note_num = 0;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                MidiMessage::NoteOff { key, .. } => {
                    handle_note_off(key, &mut note_num);
                }
                MidiMessage::PitchBend { bend } => match mode {
                    1 | 5 => {
                        let out = (bend.as_f32() * bend_range as f32 * 410. / 12. + 2048.) as u16;
                        offset_glob.set(out);
                        leds.set(
                            0,
                            Led::Top,
                            led_color,
                            Brightness::Custom((bend.as_f32() * 255.0) as u8),
                        );
                        leds.set(
                            0,
                            Led::Bottom,
                            led_color,
                            Brightness::Custom((bend.as_f32() * -255.0) as u8),
                        );
                    }
                    _ => {}
                },
                MidiMessage::ChannelAftertouch { vel } => {
                    if mode == 4 {
                        let val = scale_bits_7_12(vel);
                        offset_glob.set(val);
                    }
                }

                _ => {}
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    let muted = storage.query(|s| s.muted);
                    muted_glob.set(muted);
                    if muted {
                        leds.unset(0, Led::Button);
                    } else {
                        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
                    }
                }
                SceneEvent::SaveScene(scene) => storage.save_to_scene(scene).await,
            }
        }
    };

    join5(
        output_handler,
        button_handler,
        fader_handler,
        midi_handler,
        scene_handler,
    )
    .await;
}
