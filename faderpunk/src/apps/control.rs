use embassy_futures::{
    join::join4,
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{attenuate, attenuate_bipolar, clickless, split_unsigned_value},
    AppIcon, Brightness, Color, MidiCc, MidiChannel, MidiOut, APP_MAX_PARAMS,
};
use serde::{Deserialize, Serialize};

use libfp::{Config, Curve, Param, Range, Value};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 12;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Control",
    "Simple MIDI/CV controller",
    Color::Violet,
    AppIcon::Fader,
)
.add_param(Param::Curve {
    name: "Curve",
    variants: &[Curve::Linear, Curve::Logarithmic, Curve::Exponential],
})
.add_param(Param::Range {
    name: "Range",
    variants: &[Range::_0_10V, Range::_Neg5_5V],
})
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiCc { name: "MIDI CC" })
.add_param(Param::bool {
    name: "Mute on release",
})
.add_param(Param::bool { name: "Invert" })
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
.add_param(Param::Enum {
    name: "Button mode",
    variants: &["Mute", "CC toggle", "CC momentary"],
})
.add_param(Param::MidiChannel {
    name: "Button Channel",
})
.add_param(Param::MidiCc { name: "Button CC" })
.add_param(Param::MidiOut);

pub struct Params {
    curve: Curve,
    range: Range,
    midi_channel: MidiChannel,
    midi_cc: MidiCc,
    midi_out: MidiOut,
    on_release: bool,
    invert: bool,
    color: Color,
    save_state: bool,
    button_mode: usize,
    button_ch: MidiChannel,
    button_cc: MidiCc,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            curve: Curve::Linear,
            range: Range::_0_10V,
            midi_channel: MidiChannel::default(),
            midi_cc: MidiCc::from(32),
            midi_out: MidiOut::default(),
            on_release: false,
            invert: false,
            color: Color::Violet,
            save_state: true,
            button_mode: 0,
            button_ch: MidiChannel::default(),
            button_cc: MidiCc::from(33),
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
            midi_cc: MidiCc::from_value(values[3]),
            on_release: bool::from_value(values[4]),
            invert: bool::from_value(values[5]),
            color: Color::from_value(values[6]),
            save_state: bool::from_value(values[7]),
            button_mode: usize::from_value(values[8]),
            button_ch: MidiChannel::from_value(values[9]),
            button_cc: MidiCc::from_value(values[10]),
            midi_out: MidiOut::from_value(values[11]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.curve.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(self.on_release.into()).unwrap();
        vec.push(self.invert.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.save_state.into()).unwrap();
        vec.push(self.button_mode.into()).unwrap();
        vec.push(self.button_ch.into()).unwrap();
        vec.push(self.button_cc.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    muted: bool,
    att_saved: u16,
    fad_val: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            muted: false,
            att_saved: 4095,
            fad_val: 4095,
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
        midi_chan,
        midi_cc,
        midi_out,
        on_release,
        range,
        inverted,
        led_color,
        save_state,
        button_mode,
        button_ch,
        button_cc,
    ) = params.query(|p| {
        (
            p.curve,
            p.midi_channel,
            p.midi_cc,
            p.midi_out,
            p.on_release,
            p.range,
            p.invert,
            p.color,
            p.save_state,
            p.button_mode,
            p.button_ch,
            p.button_cc,
        )
    });

    let buttons = app.use_buttons();
    let fader = app.use_faders();
    let leds = app.use_leds();
    let midi = app.use_midi_output(midi_out, midi_chan);
    let midi_button = app.use_midi_output(midi_out, button_ch);
    let i2c = app.use_i2c_output();

    let muted_glob = app.make_global(storage.query(|s| s.muted));
    let output_glob = app.make_global(0);
    let latch_layer_glob = app.make_global(LatchLayer::Main);

    if muted_glob.get() {
        leds.unset(0, Led::Button);
    } else {
        leds.set(0, Led::Button, led_color, Brightness::Mid);
    }

    let bipolar = range.is_bipolar();

    let jack = if !bipolar {
        app.make_out_jack(0, Range::_0_10V).await
    } else {
        app.make_out_jack(0, Range::_Neg5_5V).await
    };

    let main_loop = async {
        let mut latch = app.make_latch(fader.get_value());
        let mut main_layer_value = fader.get_value();
        let mut fad_val = 0;
        let mut out = 0;
        let mut last_out = 0;

        loop {
            app.delay_millis(1).await;

            let latch_active_layer =
                latch_layer_glob.set(LatchLayer::from(buttons.is_shift_pressed()));
            let att_layer_value = storage.query(|s| s.att_saved);
            if save_state {
                main_layer_value = storage.query(|s| s.fad_val);
            }

            let latch_target_value = match latch_active_layer {
                LatchLayer::Main => main_layer_value,
                LatchLayer::Alt => att_layer_value,
                LatchLayer::Third => 0,
            };

            if let Some(new_value) =
                latch.update(fader.get_value(), latch_active_layer, latch_target_value)
            {
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

            // Calculate output values
            let muted = if button_mode != 0 {
                false
            } else {
                muted_glob.get()
            };
            let att_layer_value = storage.query(|s| s.att_saved);
            let val = if muted {
                if bipolar {
                    2047
                } else {
                    0
                }
            } else if !bipolar {
                fad_val = clickless(fad_val, curve.at(main_layer_value));
                fad_val
            } else if main_layer_value > 2047 {
                fad_val = clickless(fad_val, curve.at((main_layer_value - 2047) * 2) / 2 + 2047);
                fad_val
            } else {
                fad_val = clickless(fad_val, 2047 - curve.at((2047 - main_layer_value) * 2) / 2);
                fad_val
            };
            let mut attenuated = if bipolar {
                attenuate_bipolar(val, att_layer_value)
            } else {
                ((val as u32 * att_layer_value as u32) / 4095) as u16
            };
            if inverted {
                attenuated = 4095 - attenuated;
            }
            out = slew_2(out, attenuated, 3);
            jack.set_value(out);

            let midi_out = if muted {
                if bipolar {
                    2047
                } else {
                    0
                }
            } else if !bipolar {
                attenuate(main_layer_value, att_layer_value)
            } else {
                attenuate_bipolar(main_layer_value, att_layer_value)
            };
            if last_out != (midi_out as u32 * 127) / 4095 {
                midi.send_cc(midi_cc, midi_out).await;
            }
            last_out = (midi_out as u32 * 127) / 4095;

            // Update LEDs
            match latch_active_layer {
                LatchLayer::Main => {
                    if bipolar {
                        let led1 = split_unsigned_value(out);
                        leds.set(0, Led::Top, led_color, Brightness::Custom(led1[0]));
                        leds.set(0, Led::Bottom, led_color, Brightness::Custom(led1[1]));
                    } else {
                        leds.set(
                            0,
                            Led::Top,
                            led_color,
                            Brightness::Custom((attenuated as f32 / 16.) as u8),
                        );
                        leds.unset(0, Led::Bottom);
                    }
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
                }
                _ => unreachable!(),
            }
        }
    };

    let button_handler = async {
        loop {
            if button_mode == 2 {
                // Momentary mode: handle both press and release
                buttons.wait_for_down(0).await;
                leds.set(0, Led::Button, led_color, Brightness::Mid);
                midi_button.send_cc(button_cc, 4095).await;

                buttons.wait_for_up(0).await;
                leds.unset(0, Led::Button);
                midi_button.send_cc(button_cc, 0).await;
            } else {
                // Mode 0 (Mute) or Mode 1 (CC toggle): toggle on configured edge
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
                    if button_mode == 1 {
                        midi_button.send_cc(button_cc, 0).await;
                    }
                } else {
                    leds.set(0, Led::Button, led_color, Brightness::Mid);
                    if button_mode == 1 {
                        midi_button.send_cc(button_cc, 4095).await;
                    }
                }
            }
        }
    };

    let fader_event_handler = async {
        loop {
            fader.wait_for_any_change().await;

            match latch_layer_glob.get() {
                LatchLayer::Main => {
                    let out = output_glob.get();
                    // Send MIDI & I2C messages

                    i2c.send_fader_value(0, out).await;
                }
                LatchLayer::Alt => {
                    // Now we commit to storage
                    storage.save().await;
                }
                _ => unreachable!(),
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
                }
                SceneEvent::SaveScene(scene) => storage.save_to_scene(scene).await,
            }
        }
    };

    join4(
        main_loop,
        button_handler,
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
