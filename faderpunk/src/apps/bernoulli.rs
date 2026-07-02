use embassy_futures::{
    join::join5,
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue, AppIcon, Brightness, ClockDivision, Color, Config, Curve, MidiChannel,
    MidiNote, MidiOut, Param, Value, APP_MAX_PARAMS,
};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 6;

const LED_BRIGHTNESS: Brightness = Brightness::Mid;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Bernoulli Gate",
    "Two-output Bernoulli gate synced to internal clock",
    Color::Cyan,
    AppIcon::Die,
)
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiNote { name: "MIDI Note A" })
.add_param(Param::MidiNote { name: "MIDI Note B" })
.add_param(Param::i32 {
    name: "GATE %",
    min: 1,
    max: 100,
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
.add_param(Param::MidiOut);

pub struct Params {
    midi_channel: MidiChannel,
    note_a: MidiNote,
    note_b: MidiNote,
    midi_out: MidiOut,
    gatel: i32,
    color: Color,
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            midi_channel: MidiChannel::from_value(values[0]),
            note_a: MidiNote::from_value(values[1]),
            note_b: MidiNote::from_value(values[2]),
            gatel: i32::from_value(values[3]),
            color: Color::from_value(values[4]),
            midi_out: MidiOut::from_value(values[5]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.note_a.into()).unwrap();
        vec.push(self.note_b.into()).unwrap();
        vec.push(self.gatel.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    div_saved: u16,
    muted: bool,
    prob_saved: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            div_saved: 3000,
            muted: false,
            prob_saved: 2048,
        }
    }
}
impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16 / CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            midi_channel: MidiChannel::default(),
            note_a: MidiNote::from(36),
            note_b: MidiNote::from(37),
            midi_out: MidiOut([false, false, false]),
            gatel: 50,
            color: Color::Cyan,
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
    let (midi_out, midi_chan, note_a, note_b, gatel, led_color) = params.query(|p| {
        (
            p.midi_out,
            p.midi_channel,
            p.note_a,
            p.note_b,
            p.gatel as u32,
            p.color,
        )
    });

    let curve = Curve::Linear;

    let mut clock = app.use_clock();
    let ticks = clock.get_ticker();
    let die = app.use_die();
    let faders = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    let midi = app.use_midi_output(midi_out, midi_chan, false);

    let glob_muted = app.make_global(false);
    let div_glob = app.make_global(6);
    let prob_glob = app.make_global(2048_u16);

    let jack_a = app.make_gate_jack(0, 4095).await;
    let jack_b = app.make_gate_jack(1, 4095).await;

    let resolution = [384, 192, 96, 48, 24, 16, 12, 8, 6, 4, 3, 2];

    let (div_saved, muted, prob_saved) =
        storage.query(|s| (s.div_saved, s.muted, s.prob_saved));

    glob_muted.set(muted);
    prob_glob.set(prob_saved);
    div_glob.set(resolution[div_saved as usize / 345]);

    if muted {
        leds.unset_all();
    } else {
        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
        leds.set(1, Led::Button, led_color, LED_BRIGHTNESS);
    }

    let fut_clock = async {
        let mut note_on_a = false;
        let mut note_on_b = false;
        let mut active_out: Option<usize> = None;

        let mut cached_div = div_glob.get();
        let mut cached_gate_step = (cached_div * gatel / 100).clamp(1, cached_div - 1);

        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset | ClockEvent::Stop => {
                    midi.send_note_off(note_a).await;
                    midi.send_note_off(note_b).await;
                    note_on_a = false;
                    note_on_b = false;
                    active_out = None;

                    jack_a.set_low().await;
                    jack_b.set_low().await;

                    leds.unset(0, Led::Top);
                    leds.unset(1, Led::Top);
                }

                ClockEvent::Tick => {
                    let div = div_glob.get();
                    if div != cached_div {
                        cached_div = div;
                        cached_gate_step = (cached_div * gatel / 100).clamp(1, cached_div - 1);
                    }

                    let clkn = ticks() as u32;

                    if clkn.is_multiple_of(cached_div) {
                        let muted = glob_muted.get();
                        let probability = prob_glob.get();
                        let roll = die.roll();

                        if !muted {
                            if curve.at(probability) >= roll {
                                jack_a.set_high().await;
                                leds.set(0, Led::Top, led_color, LED_BRIGHTNESS);
                                midi.send_note_on(note_a, 4095).await;
                                note_on_a = true;
                                active_out = Some(0);
                            } else {
                                jack_b.set_high().await;
                                leds.set(1, Led::Top, led_color, LED_BRIGHTNESS);
                                midi.send_note_on(note_b, 4095).await;
                                note_on_b = true;
                                active_out = Some(1);
                            }
                        }
                    }

                    if clkn % cached_div == cached_gate_step {
                        match active_out {
                            Some(0) => {
                                if note_on_a {
                                    midi.send_note_off(note_a).await;
                                    note_on_a = false;
                                }
                                jack_a.set_low().await;
                                leds.unset(0, Led::Top);
                            }
                            Some(1) => {
                                if note_on_b {
                                    midi.send_note_off(note_b).await;
                                    note_on_b = false;
                                }
                                jack_b.set_low().await;
                                leds.unset(1, Led::Top);
                            }
                            _ => {}
                        }

                        active_out = None;
                    }
                }

                _ => {}
            }
        }
    };

    let fut_buttons = async {
        loop {
            buttons.wait_for_any_down().await;
            let muted = glob_muted.toggle();

            storage.modify_and_save(|s| {
                s.muted = muted;
                s.muted
            });

            if muted {
                jack_a.set_low().await;
                jack_b.set_low().await;
                leds.unset_all();
            } else {
                leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
                leds.set(1, Led::Button, led_color, LED_BRIGHTNESS);
            }
        }
    };

    let fut_faders = async {
        let mut latch = [
            app.make_latch(faders.get_value_at(0)),
            app.make_latch(faders.get_value_at(1)),
        ];

        loop {
            let chan = faders.wait_for_any_change().await;

            let target_value = match chan {
                0 => storage.query(|s| s.prob_saved),
                1 => storage.query(|s| s.div_saved),
                _ => 0,
            };

            if let Some(new_value) =
                latch[chan].update(faders.get_value_at(chan), libfp::latch::LatchLayer::Main, target_value)
            {
                match chan {
                    0 => {
                        prob_glob.set(new_value);
                        storage.modify_and_save(|s| s.prob_saved = new_value);
                        leds.set(
                            0,
                            Led::Bottom,
                            led_color,
                            Brightness::Custom((new_value / 16) as u8),
                        );
                    }

                    1 => {
                        div_glob.set(resolution[new_value as usize / 345]);
                        storage.modify_and_save(|s| s.div_saved = new_value);
                        leds.set(
                            1,
                            Led::Bottom,
                            led_color,
                            Brightness::Custom((new_value / 16) as u8),
                        );
                    }

                    _ => {}
                }
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    let (div_saved, muted, prob_saved) =
                        storage.query(|s| (s.div_saved, s.muted, s.prob_saved));

                    glob_muted.set(muted);
                    prob_glob.set(prob_saved);
                    div_glob.set(resolution[div_saved as usize / 345]);

                    if muted {
                        leds.unset_all();
                        jack_a.set_low().await;
                        jack_b.set_low().await;
                    } else {
                        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
                        leds.set(1, Led::Button, led_color, LED_BRIGHTNESS);
                    }
                }

                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    let idle = async {
        loop {
            app.delay_millis(1000).await;
        }
    };

    join5(fut_clock, fut_buttons, fut_faders, scene_handler, idle).await;
}