use embassy_futures::{
    join::join5,
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{attenuate_bipolar, clickless, split_unsigned_value},
    AppIcon, Brightness, Color, MidiCc, MidiChannel, MidiOut, Waveform, APP_MAX_PARAMS,
};

use serde::{Deserialize, Serialize};

use libfp::{Config, Curve, Param, Range, Value};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 9;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Panner",
    "Use with 2 VCA to do panning or cross fading",
    Color::Blue,
    AppIcon::Stereo,
)
.add_param(Param::Curve {
    name: "Curve",
    variants: &[Curve::Linear, Curve::Logarithmic, Curve::Exponential],
})
.add_param(Param::Range {
    name: "Range",
    variants: &[Range::_0_10V, Range::_0_5V, Range::_Neg5_5V],
})
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiCc { name: "MIDI CC 1" })
.add_param(Param::MidiCc { name: "MIDI CC 2" })
.add_param(Param::bool {
    name: "Mute on release",
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
.add_param(Param::bool {
    name: "Store state",
})
.add_param(Param::MidiOut);

pub struct Params {
    curve: Curve,
    range: Range,
    midi_channel: MidiChannel,
    midi_cc_l: MidiCc,
    midi_cc_r: MidiCc,
    midi_out: MidiOut,
    on_release: bool,
    color: Color,
    save_state: bool,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            curve: Curve::Linear,
            range: Range::_0_10V,
            midi_channel: MidiChannel::default(),
            midi_cc_l: MidiCc::from(32),
            midi_cc_r: MidiCc::from(33),
            midi_out: MidiOut([false, false, false]),
            on_release: false,
            color: Color::Blue,
            save_state: true,
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            curve: Curve::from_value(values[0]),
            range: Range::from_value(values[1]),
            midi_channel: MidiChannel::from_value(values[2]),
            midi_cc_l: MidiCc::from_value(values[3]),
            midi_cc_r: MidiCc::from_value(values[4]),
            on_release: bool::from_value(values[5]),
            color: Color::from_value(values[6]),
            save_state: bool::from_value(values[7]),
            midi_out: MidiOut::from_value(values[8]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.curve.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc_l.into()).unwrap();
        vec.push(self.midi_cc_r.into()).unwrap();
        vec.push(self.on_release.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.save_state.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    muted: bool,
    att_saved: u16,
    fad_val: u16,
    pan_val: u16,
    lfo_speed: u16,
    lfo_amt: u16,
    wave: Waveform,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            muted: false,
            att_saved: 4095,
            fad_val: 4095,
            pan_val: 2048,
            lfo_speed: 2048,
            lfo_amt: 0,
            wave: Waveform::Sine,
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

pub async fn run(
    app: &App<CHANNELS>,
    params: &ParamStore<Params>,
    storage: &ManagedStorage<Storage>,
) {
    let (
        curve,
        midi_out,
        midi_chan,
        midi_cc_l,
        midi_cc_r,
        on_release,
        range,
        led_color,
        save_state,
    ) = params.query(|p| {
        (
            p.curve,
            p.midi_out,
            p.midi_channel,
            p.midi_cc_l,
            p.midi_cc_r,
            p.on_release,
            p.range,
            p.color,
            p.save_state,
        )
    });

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    let midi = app.use_midi_output(midi_out, midi_chan);
    let i2c = app.use_i2c_output();

    let muted_glob = app.make_global(storage.query(|s| s.muted));
    let output_glob = app.make_global(0);
    let latch_layer_glob = app.make_global(LatchLayer::Main);
    let glob_lfo_speed = app.make_global(0.0682);

    if muted_glob.get() {
        leds.unset(0, Led::Button);
    } else {
        leds.set(0, Led::Button, led_color, Brightness::Mid);
    }

    let bipolar = range.is_bipolar();

    let jacks = [
        app.make_out_jack(0, range).await,
        app.make_out_jack(1, range).await,
    ];

    let speed = storage.query(|s| s.lfo_speed);
    let wave_saved = storage.query(|s| s.wave);

    glob_lfo_speed.set(curve.at(speed) as f32 * 0.015 + 0.0682);

    let color = get_color_for(wave_saved);
    leds.set(1, Led::Button, color, Brightness::Mid);

    let main_loop = async {
        let mut latch = [
            app.make_latch(faders.get_value_at(0)),
            app.make_latch(faders.get_value_at(1)),
        ];

        let mut lfo_pos: f32 = 0.;

        let mut main_layer_value = faders.get_value_at(0);
        let mut out_r = 0;
        let mut out_l = 0;
        let mut last_out = [0, 0];

        let mut val_left = 0;
        let mut val_right = 0;

        loop {
            app.delay_millis(1).await;

            //Read faders value and process latching

            let latch_active_layer = if buttons.is_shift_pressed() && !buttons.is_button_pressed(0)
            {
                LatchLayer::Alt
            } else if !buttons.is_shift_pressed() && buttons.is_button_pressed(1) {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            latch_layer_glob.set(latch_active_layer);

            let att_layer_value = storage.query(|s| s.att_saved);
            if save_state {
                main_layer_value = storage.query(|s| s.fad_val);
            }

            let latch_target_value = match latch_active_layer {
                LatchLayer::Main => main_layer_value,
                LatchLayer::Alt => att_layer_value,
                LatchLayer::Third => 0,
            };

            if let Some(new_value) = latch[0].update(
                faders.get_value_at(0),
                latch_active_layer,
                latch_target_value,
            ) {
                match latch_active_layer {
                    LatchLayer::Main => {
                        if save_state {
                            storage.modify(|s| s.fad_val = new_value);
                        } else {
                            main_layer_value = new_value;
                        }
                    }
                    LatchLayer::Alt => {
                        // Update storage but don't save yet
                        storage.modify(|s| s.att_saved = new_value);
                    }
                    LatchLayer::Third => {}
                }
            }

            lfo_pos = (lfo_pos + glob_lfo_speed.get()) % 4096.0;

            let attenuation = storage.query(|s| s.lfo_amt);
            let lfo_val =
                attenuate_bipolar(storage.query(|s| s.wave).at(lfo_pos as usize), attenuation)
                    as i16
                    - 2048;

            let pan_value = (storage.query(|s| s.pan_val) as i16 + lfo_val).clamp(0, 4095) as u16;
            let muted = muted_glob.get();
            let fad_val = if !bipolar {
                main_layer_value
            } else {
                main_layer_value / 2 + 2047
            };

            // Apply panning to main_layer_value before curve
            let pan_left = ((fad_val as u32 * pan_value as u32) / 4095) as u16;
            let pan_right = ((fad_val as u32 * (4095 - pan_value as u32)) / 4095) as u16;

            // Apply curve and clickless smoothing
            val_left = if muted {
                if bipolar {
                    2047
                } else {
                    0
                }
            } else if !bipolar {
                clickless(val_left, curve.at(pan_left))
            } else if pan_left > 2047 {
                clickless(val_left, curve.at((pan_left - 2047) * 2) / 2 + 2047)
            } else {
                clickless(val_left, 2047 - curve.at((2047 - pan_left) * 2) / 2)
            };

            val_right = if muted {
                if bipolar {
                    2047
                } else {
                    0
                }
            } else if !bipolar {
                clickless(val_right, curve.at(pan_right))
            } else if pan_right > 2047 {
                clickless(val_right, curve.at((pan_right - 2047) * 2) / 2 + 2047)
            } else {
                clickless(val_right, 2047 - curve.at((2047 - pan_right) * 2) / 2)
            };

            // Attenuation
            let att_layer_value = storage.query(|s| s.att_saved);

            let out_left = if bipolar {
                attenuate_bipolar(val_left, att_layer_value)
            } else {
                ((val_left as u32 * att_layer_value as u32) / 4095) as u16
            };

            let out_right = if bipolar {
                attenuate_bipolar(val_right, att_layer_value)
            } else {
                ((val_right as u32 * att_layer_value as u32) / 4095) as u16
            };

            // Slew limiting
            out_l = slew_2(out_l, out_left, 3);
            out_r = slew_2(out_r, out_right, 3);

            // MIDI output if changed
            let scaled_out = (out_l as u32 * 127) / 4095;
            if last_out[0] != scaled_out {
                midi.send_cc(midi_cc_l, out_l).await;
            }
            let scaled_out = (out_r as u32 * 127) / 4095;
            if last_out[1] != scaled_out {
                midi.send_cc(midi_cc_r, out_r).await;
            }

            // Output to jacks
            jacks[0].set_value(out_l);
            jacks[1].set_value(out_r);

            last_out[0] = (out_l as u32 * 127) / 4095;
            last_out[1] = (out_r as u32 * 127) / 4095;

            // Update LEDs
            match latch_active_layer {
                LatchLayer::Main => {
                    if bipolar {
                        let led1 = split_unsigned_value(out_l);
                        leds.set(0, Led::Top, led_color, Brightness::Custom(led1[0]));
                        leds.set(0, Led::Bottom, led_color, Brightness::Custom(led1[1]));
                        let led1 = split_unsigned_value(out_r);
                        leds.set(1, Led::Top, led_color, Brightness::Custom(led1[0]));
                        leds.set(1, Led::Bottom, led_color, Brightness::Custom(led1[1]));
                    } else {
                        leds.set(
                            0,
                            Led::Top,
                            led_color,
                            Brightness::Custom((out_l as f32 / 16.) as u8),
                        );
                        leds.unset(1, Led::Bottom);
                        leds.set(
                            1,
                            Led::Top,
                            led_color,
                            Brightness::Custom((out_r as f32 / 16.) as u8),
                        );
                        leds.unset(0, Led::Bottom);
                    }
                    leds.set(1, Led::Button, led_color, Brightness::Mid);
                }
                LatchLayer::Alt => {
                    if bipolar {
                        leds.set(
                            0,
                            Led::Top,
                            Color::Red,
                            Brightness::Custom((att_layer_value / 16) as u8),
                        );
                        leds.set(
                            0,
                            Led::Bottom,
                            Color::Red,
                            Brightness::Custom((att_layer_value / 16) as u8),
                        );
                    } else {
                        leds.set(
                            0,
                            Led::Top,
                            Color::Red,
                            Brightness::Custom((att_layer_value / 16) as u8),
                        );
                        leds.unset(0, Led::Bottom);
                    }
                    leds.set(
                        1,
                        Led::Top,
                        Color::Red,
                        Brightness::Custom((storage.query(|s| s.lfo_amt) / 16) as u8),
                    );
                    leds.set(
                        1,
                        Led::Bottom,
                        get_color_for(storage.query(|s: &Storage| s.wave)),
                        Brightness::Custom((lfo_val / 16).max(0) as u8),
                    );

                    leds.set(
                        1,
                        Led::Button,
                        get_color_for(storage.query(|s: &Storage| s.wave)),
                        Brightness::Mid,
                    );
                }
                LatchLayer::Third => {
                    let led_bright = split_unsigned_value(
                        storage.query(|s: &Storage| s.wave).at(lfo_pos as usize),
                    );
                    leds.set(
                        1,
                        Led::Top,
                        get_color_for(storage.query(|s: &Storage| s.wave)),
                        Brightness::Custom(led_bright[0]),
                    );
                    leds.set(
                        1,
                        Led::Bottom,
                        get_color_for(storage.query(|s: &Storage| s.wave)),
                        Brightness::Custom(led_bright[1]),
                    );
                }
            }
        }
    };

    let button0 = async {
        loop {
            if on_release {
                buttons.wait_for_up(0).await;
            } else {
                buttons.wait_for_down(0).await;
            }
            let muted = storage.modify_and_save(|s| {
                s.muted = !s.muted;
                s.muted
            });
            muted_glob.set(muted);
            if muted {
                leds.unset(0, Led::Button);
            } else {
                leds.set(0, Led::Button, led_color, Brightness::Mid);
            }
        }
    };

    let button1 = async {
        loop {
            buttons.wait_for_down(1).await;

            if buttons.is_shift_pressed() {
                let wave = storage.modify_and_save(|s| {
                    s.wave = s.wave.cycle();
                    s.wave
                });

                let color = get_color_for(wave);
                leds.set(1, Led::Button, color, Brightness::Mid);
            }
        }
    };

    let fader_event_handler = async {
        let mut latch = app.make_latch(faders.get_value_at(1));
        loop {
            let chan = faders.wait_for_any_change().await;
            if chan == 0 {
                match latch_layer_glob.get() {
                    LatchLayer::Main => {
                        let out = output_glob.get();
                        i2c.send_fader_value(0, out).await;
                    }
                    LatchLayer::Alt => {
                        // Now we commit to storage
                        storage.save().await;
                    }
                    LatchLayer::Third => {}
                }
            }
            if chan == 1 {
                let target_value = match latch_layer_glob.get() {
                    LatchLayer::Main => storage.query(|s| s.pan_val),
                    LatchLayer::Alt => storage.query(|s| s.lfo_amt),
                    LatchLayer::Third => storage.query(|s| s.lfo_speed),
                };
                if let Some(new_value) =
                    latch.update(faders.get_value_at(1), latch_layer_glob.get(), target_value)
                {
                    match latch_layer_glob.get() {
                        LatchLayer::Main => {
                            storage.modify_and_save(|s| s.pan_val = new_value);
                        }
                        LatchLayer::Alt => {
                            storage.modify_and_save(|s| s.lfo_amt = new_value);
                        }
                        LatchLayer::Third => {
                            glob_lfo_speed.set(curve.at(new_value) as f32 * 0.015 + 0.0682);
                            storage.modify_and_save(|s| s.lfo_speed = new_value);
                        }
                    }
                }
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    if save_state {
                        let muted = storage.query(|s| s.muted);
                        muted_glob.set(muted);
                        if muted {
                            leds.unset(0, Led::Button);
                        } else {
                            leds.set(0, Led::Button, led_color, Brightness::Mid);
                        }
                    }

                    let speed = storage.query(|s| s.lfo_speed);
                    let wave_saved = storage.query(|s| s.wave);

                    glob_lfo_speed.set(curve.at(speed) as f32 * 0.015 + 0.0682);

                    let color = get_color_for(wave_saved);
                    leds.set(1, Led::Button, color, Brightness::Mid);
                }
                SceneEvent::SaveScene(scene) => storage.save_to_scene(scene).await,
            }
        }
    };

    join5(
        main_loop,
        button0,
        button1,
        fader_event_handler,
        scene_handler,
    )
    .await;
}

pub fn slew_2(prev: u16, input: u16, slew: u16) -> u16 {
    // Integer-based smoothing
    let smoothed = ((prev as u32 * slew as u32 + input as u32) / (slew as u32 + 1)) as u16;

    // Snap to target if close enough
    if (smoothed as i32 - input as i32).abs() <= slew as i32 {
        input
    } else {
        smoothed
    }
}

fn get_color_for(wave: Waveform) -> Color {
    match wave {
        Waveform::Sine => Color::Yellow,
        Waveform::Triangle => Color::Pink,
        Waveform::Saw => Color::Cyan,
        Waveform::SawInv => Color::Red,
        Waveform::Square => Color::White,
    }
}
