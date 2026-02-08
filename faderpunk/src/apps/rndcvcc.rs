// Todo :
// Save div, mute, attenuation - Added the saving slots, need to add write/read in the app.
// Add attenuator (shift + fader)

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

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 4;

const LED_COLOR: Color = Color::Violet;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Random CC/CV",
    "Generate random CC and CV values",
    Color::Green,
    AppIcon::Random,
)
.add_param(Param::bool { name: "Bipolar" })
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiCc { name: "MIDI CC" })
.add_param(Param::MidiOut);

pub struct Params {
    bipolar: bool,
    midi_channel: MidiChannel,
    midi_cc: MidiCc,
    midi_out: MidiOut,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            bipolar: false,
            midi_channel: MidiChannel::default(),
            midi_cc: MidiCc::from(32),
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
            bipolar: bool::from_value(values[0]),
            midi_channel: MidiChannel::from_value(values[1]),
            midi_cc: MidiCc::from_value(values[2]),
            midi_out: MidiOut::from_value(values[3]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.bipolar.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
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
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            fader_saved: 3000,
            mute_save: false,
            att_saved: 4096,
            slew_saved: 0,
            clocked: true,
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
    let (bipolar, midi_out, midi_chan, midi_cc) =
        params.query(|p| (p.bipolar, p.midi_out, p.midi_channel, p.midi_cc));

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
    let output = app.make_out_jack(0, range).await;

    let glob_muted = app.make_global(false);
    let div_glob = app.make_global(6);
    let val_glob = app.make_global(0);
    let glob_button_color = app.make_global(Color::White);
    let time_div = app.make_global(125);

    let latched_glob = app.make_global(false);
    let glob_latch_layer = app.make_global(LatchLayer::Main);

    let resolution = [384, 192, 96, 48, 24, 16, 12, 8, 6, 4, 3, 2];

    let mut clkn = 0;

    let curve = Curve::Exponential;
    let fader_curve = Curve::Exponential;

    let (res, mute) = storage.query(|s| (s.fader_saved, s.mute_save));

    glob_muted.set(mute);
    div_glob.set(resolution[res as usize / 345]);
    if mute {
        leds.unset(0, Led::Button);
        output.set_value(2047);
        leds.unset(0, Led::Top);
        leds.unset(0, Led::Bottom);
    } else {
        leds.set(0, Led::Button, LED_COLOR, Brightness::Mid);
    }

    let fut1 = async {
        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset => {
                    clkn = 0;
                }
                ClockEvent::Tick => {
                    let muted = glob_muted.get();

                    let div = div_glob.get();
                    if clkn % div == 0 && !muted && storage.query(|s: &Storage| s.clocked) {
                        val_glob.set(rnd.roll());

                        let color = if !glob_muted.get() {
                            let r = (rnd.roll() / 16) as u8;
                            let g = (rnd.roll() / 16) as u8;
                            let b = (rnd.roll() / 16) as u8;

                            Color::Custom(r, g, b)
                        } else {
                            Color::Custom(0, 0, 0)
                        };
                        glob_button_color.set(color);

                        leds.set(0, Led::Button, color, Brightness::Mid);
                    }

                    if clkn % div == 0 && storage.query(|s: &Storage| s.clocked) && buttons.is_shift_pressed() {
                        leds.set(0, Led::Bottom, Color::Red, Brightness::High);
                    }
                    if clkn % div == (div * 50 / 100).clamp(1, div - 1)
                        && buttons.is_shift_pressed()
                    {
                        leds.unset(0, Led::Bottom);
                    }
                    clkn += 1;
                }
                _ => {}
            }
        }
    };

    let fut2 = async {
        loop {
            buttons.wait_for_any_down().await;
            if buttons.is_shift_pressed() {
                let muted = glob_muted.toggle();

                storage.modify_and_save(|s| {
                    s.mute_save = muted;
                });

                if muted {
                    leds.unset_all();
                } else {
                    leds.set(0, Led::Button, LED_COLOR, Brightness::Mid);
                }
            }
        }
    };
    let long_press = async {
        loop {
            buttons.wait_for_any_long_press().await;

            if buttons.is_shift_pressed() {
                let clocked = storage.query(|s: &Storage| s.clocked);

                let muted = glob_muted.toggle();
                storage.modify_and_save(|s| {
                    s.clocked = !clocked;
                    s.mute_save = muted;
                });
                if muted {
                    leds.unset_all();
                } else {
                    leds.set(0, Led::Button, LED_COLOR, Brightness::Mid);
                }
            }
        }
    };

    let fut3 = async {
        let mut latch = app.make_latch(fader.get_value());
        loop {
            fader.wait_for_change_at(0).await;

            let latch_layer = glob_latch_layer.get();

            let target_value = match latch_layer {
                LatchLayer::Main => storage.query(|s| s.fader_saved),
                LatchLayer::Alt => storage.query(|s| s.att_saved),
                LatchLayer::Third => storage.query(|s| s.slew_saved),
            };

            if let Some(new_value) = latch.update(fader.get_value(), latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Main => {
                        div_glob.set(resolution[new_value as usize / 345]);
                        time_div.set((curve.at(4095 - new_value) as u32 * 5000 / 4095 + 71) as u16);
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
                        leds.unset(0, Led::Button);
                        leds.unset(0, Led::Top);
                        leds.unset(0, Led::Bottom);
                    } else {
                        leds.set(0, Led::Button, LED_COLOR, Brightness::Mid);
                    }
                    latched_glob.set(false);
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
        loop {
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

            let att = storage.query(|s| s.att_saved);

            let jackval = if bipolar {
                attenuate_bipolar(val_glob.get(), att)
            } else {
                attenuate(val_glob.get(), att)
            };

            out = if !glob_muted.get() {
                slew_2(
                    out,
                    jackval,
                    fader_curve.at(storage.query(|s| s.slew_saved)),
                )
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
                let color = glob_button_color.get();
                if bipolar {
                    let ledj = split_unsigned_value(out as u16);
                    leds.set(0, Led::Top, color, Brightness::Custom(ledj[0]));
                    leds.set(0, Led::Bottom, color, Brightness::Custom(ledj[1]));
                } else {
                    leds.set(
                        0,
                        Led::Top,
                        color,
                        Brightness::Custom((last_out / 16) as u8),
                    );
                }
            }
            if latch_active_layer == LatchLayer::Alt {
                leds.set(
                    0,
                    Led::Top,
                    Color::Red,
                    Brightness::Custom((att / 16) as u8),
                );
                // if storage.query(|s: &Storage| s.clocked) {
                //     leds.set(0, Led::Bottom, Color::Red, Brightness::High);
                // }
            }
            if latch_active_layer == LatchLayer::Third {
                leds.set(
                    0,
                    Led::Top,
                    Color::Green,
                    Brightness::Custom((storage.query(|s| s.slew_saved) / 16) as u8),
                );
            }
            if !storage.query(|s: &Storage| s.clocked) {
                count += 1;
                if count.is_multiple_of(time_div.get() as u32) {
                    val_glob.set(rnd.roll());

                    let color = if !glob_muted.get() {
                        let r = (rnd.roll() / 16) as u8;
                        let g = (rnd.roll() / 16) as u8;
                        let b = (rnd.roll() / 16) as u8;

                        Color::Custom(r, g, b)
                    } else {
                        Color::Custom(0, 0, 0)
                    };
                    glob_button_color.set(color);

                    leds.set(0, Led::Button, color, Brightness::Mid);
                }
            }
        }
    };

    join(
        long_press,
        join5(fut1, fut2, fut3, scene_handler, timed_loop),
    )
    .await;
}

fn slew_2(prev: f32, input: u16, slew: u16) -> f32 {
    (prev * slew as f32 + input as f32) / (slew + 1) as f32
}
