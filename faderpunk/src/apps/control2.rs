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
    AppIcon, Brightness, Color, MidiCc, MidiChannel, MidiOut, APP_MAX_PARAMS,
};
use serde::{Deserialize, Serialize};

use libfp::{Config, Curve, Param, Range, Value};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 9;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Control X2",
    "Simple MIDI/CV controller with CC button",
    Color::Green,
    AppIcon::Fader,
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
    name: "Channel Fader",
})
.add_param(Param::MidiCc { name: "CC Fader" })
.add_param(Param::MidiChannel {
    name: "Channel Button",
})
.add_param(Param::MidiCc { name: "CC Button" })
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
    midi_channel_fader: MidiChannel,
    midi_cc_fader: MidiCc,
    midi_channel_button: MidiChannel,
    midi_cc_button: MidiCc,
    color: Color,
    save_state: bool,
    midi_out: MidiOut,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            curve: Curve::Linear,
            range: Range::_0_10V,
            midi_channel_fader: MidiChannel::default(),
            midi_cc_fader: MidiCc::from(32),
            midi_channel_button: MidiChannel::default(),
            midi_cc_button: MidiCc::from(33),
            color: Color::Green,
            save_state: true,
            midi_out: MidiOut::default(),
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
            midi_channel_fader: MidiChannel::from_value(values[2]),
            midi_cc_fader: MidiCc::from_value(values[3]),
            midi_channel_button: MidiChannel::from_value(values[4]),
            midi_cc_button: MidiCc::from_value(values[5]),
            color: Color::from_value(values[6]),
            save_state: bool::from_value(values[7]),
            midi_out: MidiOut::from_value(values[8]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.curve.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.midi_channel_fader.into()).unwrap();
        vec.push(self.midi_cc_fader.into()).unwrap();
        vec.push(self.midi_channel_button.into()).unwrap();
        vec.push(self.midi_cc_button.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.save_state.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec
    }
}

// TODO: Make a macro to generate this.
#[derive(Serialize, Deserialize)]
pub struct Storage {
    button_state: bool,
    att_saved: u16,
    fad_val: u16,
    toggle: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            button_state: true,
            att_saved: 4095,
            fad_val: 4095,
            toggle: true,
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
        midi_channel_fader,
        midi_cc_fader,
        midi_cc_button,
        range,
        led_color,
        save_state,
        midi_channel_button,
        midi_out,
    ) = params.query(|p| {
        (
            p.curve,
            p.midi_channel_fader,
            p.midi_cc_fader,
            p.midi_cc_button,
            p.range,
            p.color,
            p.save_state,
            p.midi_channel_button,
            p.midi_out,
        )
    });

    let buttons = app.use_buttons();
    let fader = app.use_faders();
    let leds = app.use_leds();
    let midi_fader = app.use_midi_output(midi_out, midi_channel_fader);
    let midi_button = app.use_midi_output(midi_out, midi_channel_button);
    let i2c = app.use_i2c_output();

    let output_glob = app.make_global(0);
    let latch_layer_glob = app.make_global(LatchLayer::Main);
    if storage.query(|s| s.toggle) {
        if !storage.query(|s| s.button_state) {
            leds.unset(0, Led::Button)
        } else {
            leds.set(0, Led::Button, led_color, Brightness::Lower);
            midi_fader
                .send_cc(
                    midi_cc_button,
                    storage.query(|s| s.button_state) as u16 * 4095,
                )
                .await;
        }
    }

    let bipolar = range.is_bipolar();

    let jack = app.make_out_jack(0, range).await;

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
                _ => unreachable!(),
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
                    _ => unreachable!(),
                }
            }

            let val = if !bipolar {
                fad_val = clickless(fad_val, curve.at(main_layer_value));
                fad_val
            } else if main_layer_value > 2047 {
                fad_val = clickless(fad_val, curve.at((main_layer_value - 2047) * 2) / 2 + 2047);
                fad_val
            } else {
                fad_val = clickless(fad_val, 2047 - curve.at((2047 - main_layer_value) * 2) / 2);
                fad_val
            };

            let att_layer_value = storage.query(|s| s.att_saved);
            let mut attenuated = if bipolar {
                attenuate_bipolar(val, att_layer_value)
            } else {
                ((val as u32 * att_layer_value as u32) / 4095) as u16
            };
            out = slew_2(out, attenuated, 3);
            if last_out != (out as u32 * 127) / 4095 {
                midi_fader.send_cc(midi_cc_fader, out).await;
            }
            jack.set_value(out);
            last_out = (out as u32 * 127) / 4095;

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
                    if storage.query(|s| s.toggle) {
                        if storage.query(|s| s.button_state) {
                            leds.set(0, Led::Button, led_color, Brightness::Lower);
                        } else {
                            leds.unset(0, Led::Button);
                        }
                    } else {
                        if buttons.is_button_pressed(0) {
                            leds.set(0, Led::Button, led_color, Brightness::Lower);
                        } else {
                            leds.unset(0, Led::Button);
                        }
                    }
                }
                LatchLayer::Alt => {
                    if storage.query(|s| s.toggle) {
                        leds.set(0, Led::Button, Color::Red, Brightness::Lower);
                    } else {
                        leds.unset(0, Led::Button);
                    }
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

    let button_down_handler = async {
        loop {
            buttons.wait_for_down(0).await;
            if !buttons.is_shift_pressed() {
                if storage.query(|s| s.toggle) {
                    let button_state = storage.modify_and_save(|s| {
                        s.button_state = !s.button_state;
                        s.button_state
                    });
                    midi_button
                        .send_cc(midi_cc_button, button_state as u16 * 4095)
                        .await;
                    if button_state {
                        leds.set(0, Led::Button, led_color, Brightness::Lower);
                    } else {
                        leds.unset(0, Led::Button);
                    }
                } else {
                    midi_button.send_cc(midi_cc_button, 4095).await;
                    leds.set(0, Led::Button, led_color, Brightness::Lower);
                }
            } else {
                let toggle = storage.modify_and_save(|s| {
                    s.toggle = !s.toggle;
                    s.toggle
                });

                if toggle {
                    let button_state = storage.query(|s| s.button_state);
                    midi_button
                        .send_cc(midi_cc_button, button_state as u16 * 4095)
                        .await;
                }
            }
        }
    };

    let button_up_handler = async {
        loop {
            buttons.wait_for_up(0).await;
            if !storage.query(|s| s.toggle) {
                midi_button.send_cc(midi_cc_button, 0).await;
                leds.unset(0, Led::Button);
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
                SceneEvent::LoadSscene(scene) => {
                    storage.load_from_scene(scene).await;
                    if save_state {
                        if storage.query(|s| s.toggle) {
                            let button_state = storage.query(|s| s.button_state);
                            if !button_state {
                                leds.unset(0, Led::Button);
                                midi_button.send_cc(midi_cc_button, 0).await;
                            } else {
                                leds.set(0, Led::Button, led_color, Brightness::Lower);
                                midi_button
                                    .send_cc(midi_cc_button, button_state as u16 * 4095)
                                    .await;
                            }
                        }
                    }
                }
                SceneEvent::SaveScene(scene) => storage.save_to_scene(scene).await,
            }
        }
    };

    join5(
        main_loop,
        button_down_handler,
        button_up_handler,
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
