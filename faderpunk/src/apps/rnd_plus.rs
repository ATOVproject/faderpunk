use embassy_futures::{
    join::{join, join5},
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{attenuate, attenuate_bipolar, split_unsigned_value},
    AppIcon, Brightness, ClockDivision, Color, Config, Curve, MidiCc, MidiChannel, MidiOut, Param,
    Range, Value, APP_MAX_PARAMS,
};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 5;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Random+",
    "Generate random CC and CV values with assignable CV input",
    Color::Green,
    AppIcon::Random,
)
.add_param(Param::bool { name: "Bipolar" })
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiCc { name: "MIDI CC" })
.add_param(Param::MidiOut)
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
});

pub struct Params {
    bipolar: bool,
    midi_channel: MidiChannel,
    midi_cc: MidiCc,
    midi_out: MidiOut,
    color: Color,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            bipolar: false,
            midi_channel: MidiChannel::default(),
            midi_cc: MidiCc::from(32),
            midi_out: MidiOut::default(),
            color: Color::Green,
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            bipolar: bool::from_value(values[0]),
            midi_channel: MidiChannel::from_value(values[1]),
            midi_cc: MidiCc::from_value(values[2]),
            midi_out: MidiOut::from_value(values[3]),
            color: Color::from_value(values[4]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.bipolar.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    fader_saved: u16,
    mute_save: bool,
    att_saved: u16,
    slew_saved: u16,
    clocked: bool,
    in_att: u16,
    in_mute: bool,
    dest: usize,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            fader_saved: 3000,
            mute_save: false,
            att_saved: 4096,
            slew_saved: 0,
            clocked: true,
            in_att: 4095,
            in_mute: false,
            dest: 0, // 0 => speed, 1 => ext clock, 2 => slew
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
    let (bipolar, midi_out, midi_chan, midi_cc, led_color) =
        params.query(|p| (p.bipolar, p.midi_out, p.midi_channel, p.midi_cc, p.color));

    let mut clock = app.use_clock();
    let rnd = app.use_die();
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let midi = app.use_midi_output(midi_out, midi_chan);
    let range = if bipolar {
        Range::_Neg5_5V
    } else {
        Range::_0_10V
    };

    let input = app.make_in_jack(0, Range::_Neg5_5V).await;
    let output = app.make_out_jack(1, range).await;

    let glob_muted = app.make_global(false);
    let div_glob = app.make_global(6);
    let val_glob = app.make_global(0);
    let glob_button_color = app.make_global(Color::White);
    let time_div = app.make_global(125);
    let in_val_glob = app.make_global(2047);

    let glob_latch_layer = app.make_global(LatchLayer::Main);

    let resolution = [384, 192, 96, 48, 24, 16, 12, 8, 6, 4, 3, 2];

    let mut clkn = 0;

    let curve = Curve::Exponential;
    let fader_curve = Curve::Exponential;

    let (res, mute) = storage.query(|s| (s.fader_saved, s.mute_save));

    glob_muted.set(mute);
    div_glob.set(resolution[res as usize / 345]);
    if mute {
        leds.unset(1, Led::Button);
        output.set_value(2047);
        leds.unset(1, Led::Top);
        leds.unset(1, Led::Bottom);
    } else {
        leds.set(1, Led::Button, led_color, Brightness::Mid);
    }

    let fut1 = async {
        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset => {
                    clkn = 0;
                }
                ClockEvent::Tick => {
                    let muted = storage.query(|s: &Storage| s.mute_save);

                    let destination = storage.query(|s| s.dest);
                    let base_speed = storage.query(|s| s.fader_saved);
                    let div = if destination == 0 {
                        resolution_with_input_offset(base_speed, in_val_glob.get(), &resolution)
                    } else {
                        resolution[(base_speed as usize / 345).clamp(0, resolution.len() - 1)]
                    };
                    if clkn % div == 0
                        && !muted
                        && storage.query(|s: &Storage| s.clocked)
                        && destination != 1
                    {
                        val_glob.set(rnd.roll());

                        let rnd_color = if !storage.query(|s: &Storage| s.mute_save) {
                            let r = (rnd.roll() / 16) as u8;
                            let g = (rnd.roll() / 16) as u8;
                            let b = (rnd.roll() / 16) as u8;

                            Color::Custom(r, g, b)
                        } else {
                            Color::Custom(0, 0, 0)
                        };
                        glob_button_color.set(rnd_color);

                        leds.set(1, Led::Button, rnd_color, Brightness::Mid);
                    }

                    if clkn % div == 0
                        && storage.query(|s: &Storage| s.clocked)
                        && buttons.is_shift_pressed()
                    {
                        leds.set(1, Led::Bottom, Color::Red, Brightness::High);
                    }
                    if clkn % div == (div * 50 / 100).clamp(1, div - 1)
                        && buttons.is_shift_pressed()
                    {
                        leds.unset(1, Led::Bottom);
                    }
                    clkn += 1;
                }
                _ => {}
            }
        }
    };

    let short_press = async {
        loop {
            let (chan, shift) = buttons.wait_for_any_down().await;

            if chan == 0 {
                if !shift {
                    storage.modify_and_save(|s| {
                        s.in_mute = !s.in_mute;
                    });
                } else {
                    let dest = (storage.query(|s| s.dest) + 1) % 3;
                    storage.modify_and_save(|s| {
                        s.dest = dest;
                    });
                }
            }
            if chan == 1 && shift {
                let muted = !storage.query(|s: &Storage| s.mute_save);

                storage.modify_and_save(|s| {
                    s.mute_save = muted;
                });

                if muted {
                    leds.unset_all();
                } else {
                    leds.set(1, Led::Button, led_color, Brightness::Mid);
                }
            }
        }
    };
    let long_press = async {
        loop {
            buttons.wait_for_long_press(1).await;

            if buttons.is_shift_pressed() {
                let clocked = storage.query(|s: &Storage| s.clocked);

                let muted = !storage.query(|s: &Storage| s.mute_save);
                storage.modify_and_save(|s| {
                    s.clocked = !clocked;
                    s.mute_save = muted;
                });
                if muted {
                    leds.unset_all();
                } else {
                    leds.set(1, Led::Button, led_color, Brightness::Mid);
                }
            }
        }
    };

    let fader_handler = async {
        let mut latch = [
            app.make_latch(fader.get_value_at(0)),
            app.make_latch(fader.get_value_at(1)),
        ];
        loop {
            let chan = fader.wait_for_any_change().await;

            let latch_layer = glob_latch_layer.get();
            if chan == 0 {
                let target_value = match latch_layer {
                    LatchLayer::Main => storage.query(|s| s.in_att),
                    LatchLayer::Alt => 0,
                    LatchLayer::Third => 0,
                };

                if let Some(new_value) =
                    latch[chan].update(fader.get_value_at(chan), latch_layer, target_value)
                {
                    match latch_layer {
                        LatchLayer::Main => {
                            storage.modify_and_save(|s| s.in_att = new_value);
                        }
                        LatchLayer::Alt => {}
                        LatchLayer::Third => {}
                    }
                }
            }
            if chan == 1 {
                let target_value = match latch_layer {
                    LatchLayer::Main => storage.query(|s| s.fader_saved),
                    LatchLayer::Alt => storage.query(|s| s.att_saved),
                    LatchLayer::Third => storage.query(|s| s.slew_saved),
                };

                if let Some(new_value) =
                    latch[chan].update(fader.get_value_at(chan), latch_layer, target_value)
                {
                    match latch_layer {
                        LatchLayer::Main => {
                            div_glob.set(resolution[new_value as usize / 345]);
                            time_div
                                .set((curve.at(4095 - new_value) as u32 * 5000 / 4095 + 71) as u16);
                            storage.modify_and_save(|s| s.fader_saved = new_value);
                        }
                        LatchLayer::Alt => {
                            storage.modify_and_save(|s| s.att_saved = new_value);
                        }
                        LatchLayer::Third => {
                            storage.modify_and_save(|s| s.slew_saved = new_value);
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
                    let (res, mute, _) =
                        storage.query(|s| (s.fader_saved, s.mute_save, s.att_saved));

                    glob_muted.set(mute);
                    div_glob.set(resolution[res as usize / 345]);
                    if mute {
                        leds.unset(1, Led::Button);
                        leds.unset(1, Led::Top);
                        leds.unset(1, Led::Bottom);
                    }
                }

                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    let timed_loop = async {
        let mut out = 0.;
        let mut last_out = 0;
        let mut count: u32 = 0;
        let mut oldinputval = 0;
        loop {
            app.delay_millis(1).await;
            let latch_active_layer = if buttons.is_shift_pressed() && !buttons.is_button_pressed(1)
            {
                LatchLayer::Alt
            } else if !buttons.is_shift_pressed() && buttons.is_button_pressed(1) {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            glob_latch_layer.set(latch_active_layer);

            let in_mute = storage.query(|s| s.in_mute);
            let in_val = if in_mute {
                2047
            } else {
                attenuate_bipolar(input.get_value(), storage.query(|s| s.in_att))
            };
            let led_in = split_unsigned_value(in_val);
            leds.set(0, Led::Top, led_color, Brightness::Custom(led_in[0]));
            leds.set(0, Led::Bottom, led_color, Brightness::Custom(led_in[1]));
            in_val_glob.set(in_val);

            let destination = storage.query(|s| s.dest);

            if destination == 1 {
                if in_val >= 2458 && oldinputval < 2458 {
                    val_glob.set(rnd.roll());

                    let rnd_color = if !storage.query(|s: &Storage| s.mute_save) {
                        let r = (rnd.roll() / 16) as u8;
                        let g = (rnd.roll() / 16) as u8;
                        let b = (rnd.roll() / 16) as u8;
                        Color::Custom(r, g, b)
                    } else {
                        Color::Custom(0, 0, 0)
                    };
                    glob_button_color.set(rnd_color);
                    leds.set(1, Led::Button, rnd_color, Brightness::Mid);
                }
                oldinputval = in_val;
            } else {
                oldinputval = 0;
            }

            let att = storage.query(|s| s.att_saved);

            let base_slew = storage.query(|s| s.slew_saved);
            let slew = if destination == 2 {
                mod_with_input(base_slew, in_val)
            } else {
                base_slew
            };

            let jackval = if bipolar {
                attenuate_bipolar(val_glob.get(), att)
            } else {
                attenuate(val_glob.get(), att)
            };

            out = if !storage.query(|s: &Storage| s.mute_save) {
                slew_2(out, jackval, fader_curve.at(slew))
            } else if bipolar {
                2047.0
            } else {
                0.0
            };

            output.set_value(out as u16);

            if last_out / 32 != out as u16 / 32 {
                midi.send_cc(midi_cc, out as u16).await;
            }
            last_out = out as u16;

            if latch_active_layer == LatchLayer::Main {
                if storage.query(|s| s.in_mute) {
                    leds.unset(0, Led::Button);
                } else {
                    leds.set(0, Led::Button, led_color, Brightness::Mid);
                }

                let rnd_color = glob_button_color.get();

                if bipolar {
                    let ledj = split_unsigned_value(out as u16);
                    leds.set(1, Led::Top, rnd_color, Brightness::Custom(ledj[0]));
                    leds.set(1, Led::Bottom, rnd_color, Brightness::Custom(ledj[1]));
                } else {
                    leds.set(
                        1,
                        Led::Top,
                        rnd_color,
                        Brightness::Custom((last_out / 16) as u8),
                    );
                }
            }
            if latch_active_layer == LatchLayer::Alt {
                let dest_color = match destination {
                    0 => Color::Yellow,
                    1 => Color::Pink,
                    2 => Color::Cyan,
                    _ => Color::Yellow,
                };
                leds.set(0, Led::Button, dest_color, Brightness::Mid);
                leds.set(
                    1,
                    Led::Top,
                    Color::Red,
                    Brightness::Custom((storage.query(|s| s.att_saved) / 16) as u8),
                );
            }
            if latch_active_layer == LatchLayer::Third {
                leds.set(
                    1,
                    Led::Top,
                    Color::Green,
                    Brightness::Custom((storage.query(|s| s.slew_saved) / 16) as u8),
                );
            }
            if !storage.query(|s: &Storage| s.clocked) {
                count += 1;
                let base_speed = storage.query(|s| s.fader_saved);
                let speed = if destination == 0 {
                    mod_with_input(base_speed, in_val)
                } else {
                    base_speed
                };
                let timed_div = (curve.at(4095 - speed) as u32 * 5000 / 4095 + 71) as u16;

                if destination != 1 && count.is_multiple_of(timed_div as u32) {
                    val_glob.set(rnd.roll());

                    let rnd_color = if !storage.query(|s: &Storage| s.mute_save) {
                        let r = (rnd.roll() / 16) as u8;
                        let g = (rnd.roll() / 16) as u8;
                        let b = (rnd.roll() / 16) as u8;

                        Color::Custom(r, g, b)
                    } else {
                        Color::Custom(0, 0, 0)
                    };
                    glob_button_color.set(rnd_color);

                    leds.set(1, Led::Button, rnd_color, Brightness::Mid);
                }
            }
        }
    };

    join(
        long_press,
        join5(fut1, short_press, fader_handler, scene_handler, timed_loop),
    )
    .await;
}

fn slew_2(prev: f32, input: u16, slew: u16) -> f32 {
    (prev * slew as f32 + input as f32) / (slew + 1) as f32
}

fn mod_with_input(base: u16, in_val: u16) -> u16 {
    (base as i32 + in_val as i32 - 2047).clamp(0, 4095) as u16
}

fn resolution_with_input_offset(base: u16, in_val: u16, resolution: &[u16; 12]) -> u16 {
    let base_index = (base as usize / 345).clamp(0, resolution.len() - 1) as i32;
    let offset = ((in_val as i32 - 2047) * 6 / 2047).clamp(-6, 6);
    let index = (base_index + offset).clamp(0, (resolution.len() - 1) as i32) as usize;
    resolution[index]
}
