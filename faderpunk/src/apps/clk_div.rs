use embassy_futures::{
    join::join5,
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{rescale_12bit_int, resolution_for_mode, value_to_resolution},
    AppIcon, Brightness, ClockDivision, Color, Config, MidiChannel, MidiNote, MidiOut, Param,
    Value, APP_MAX_PARAMS,
};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 6;

const LED_BRIGHTNESS: Brightness = Brightness::Mid;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Clock Divider",
    "Simple clock divider",
    Color::Orange,
    AppIcon::NoteBox,
)
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiNote { name: "MIDI Note" })
.add_param(Param::i32 {
    name: "GATE %",
    min: 1,
    max: 100,
})
.add_param(Param::Enum {
    name: "Divisions",
    variants: &["Straight", "Triplets", "Both"],
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
    midi_out: MidiOut,
    note: MidiNote,
    gatel: i32,
    division_mode: usize,
    color: Color,
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }

        Some(Self {
            midi_channel: MidiChannel::from_value(values[0]),
            note: MidiNote::from_value(values[1]),
            gatel: i32::from_value(values[2]),
            division_mode: usize::from_value(values[3]),
            color: Color::from_value(values[4]),
            midi_out: MidiOut::from_value(values[5]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.note.into()).unwrap();
        vec.push(self.gatel.into()).unwrap();
        vec.push(self.division_mode.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    fader_saved: u16,
    mute_saved: bool,
    max_div: u16,
    min_div: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            fader_saved: 3000,
            mute_saved: false,
            max_div: 4095,
            min_div: 0,
        }
    }
}
impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            midi_channel: MidiChannel::default(),
            midi_out: MidiOut([false, false, false]),
            note: MidiNote::from(32),
            gatel: 50,
            division_mode: 2,
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
    let (midi_out, midi_chan, note, gatel, division_mode, led_color) = params.query(|p| {
        (
            p.midi_out,
            p.midi_channel,
            p.note,
            p.gatel as u32,
            p.division_mode,
            p.color,
        )
    });

    let mut clock = app.use_clock();
    let ticks = clock.get_ticker();
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    let midi = app.use_midi_output(midi_out, midi_chan, false);

    let glob_muted = app.make_global(false);
    let div_glob = app.make_global(6_u32);
    let max_glob = app.make_global(6_u32);
    let min_glob = app.make_global(6_u32);
    let glob_latch_layer = app.make_global(LatchLayer::Main);

    let jack = app.make_gate_jack(0, 4095).await;

    let resolution = resolution_for_mode(division_mode);

    let (res, mute, min, max) =
        storage.query(|s| (s.fader_saved, s.mute_saved, s.min_div, s.max_div));

    min_glob.set(value_to_resolution(min, resolution));
    max_glob.set(value_to_resolution(max, resolution));

    glob_muted.set(mute);
    div_glob.set(value_to_resolution(res, resolution));
    if mute {
        leds.unset(0, Led::Button);
        leds.unset(0, Led::Top);
        leds.unset(0, Led::Bottom);
    } else {
        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
    }

    let fut1 = async {
        let mut note_on = false;
        let mut cached_div = div_glob.get();
        let mut cached_gate_step = (cached_div * gatel / 100).clamp(1, cached_div - 1);

        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset => {
                    midi.send_note_off(note).await;
                    note_on = false;
                    jack.set_low().await;
                }
                ClockEvent::Stop => {
                    midi.send_note_off(note).await;
                    note_on = false;
                    jack.set_low().await;
                }
                ClockEvent::Tick => {
                    let muted = glob_muted.get();
                    let div = div_glob.get();
                    if div != cached_div {
                        cached_div = div;
                        cached_gate_step = (cached_div * gatel / 100).clamp(1, cached_div - 1);
                    }
                    let clkn = ticks() as u32;

                    if clkn.is_multiple_of(cached_div) && !muted {
                        jack.set_high().await;
                        if glob_latch_layer.get() == LatchLayer::Main {
                            if matches!(div, 2 | 4 | 8 | 16) {
                                leds.set(0, Led::Bottom, Color::Orange, Brightness::High);
                            } else {
                                leds.set(0, Led::Bottom, Color::Blue, Brightness::High);
                            }
                        }
                        midi.send_note_on(note, 4095).await;
                        note_on = true;
                    }

                    if clkn % cached_div == cached_gate_step {
                        if note_on {
                            midi.send_note_off(note).await;

                            note_on = false;
                            jack.set_low().await;
                        }
                        if glob_latch_layer.get() == LatchLayer::Main {
                            leds.set(0, Led::Top, led_color, Brightness::Off);
                            leds.set(0, Led::Bottom, led_color, Brightness::Off);
                        }
                    }

                    if glob_latch_layer.get() != LatchLayer::Main {
                        if clkn % max_glob.get()
                            == (max_glob.get() * gatel / 100).clamp(1, cached_div - 1)
                        {
                            leds.set(0, Led::Top, led_color, Brightness::Off);
                        }
                        if clkn % min_glob.get()
                            == (min_glob.get() * gatel / 100).clamp(1, cached_div - 1)
                        {
                            leds.set(0, Led::Bottom, led_color, Brightness::Off);
                        }

                        if clkn.is_multiple_of(max_glob.get()) {
                            leds.set(0, Led::Top, Color::Red, LED_BRIGHTNESS);
                        }

                        if clkn.is_multiple_of(min_glob.get()) {
                            leds.set(0, Led::Bottom, Color::Red, LED_BRIGHTNESS);
                        }
                    }
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
                    s.mute_saved = muted;
                    s.mute_saved
                });

                if muted {
                    jack.set_low().await;
                    leds.unset(0, Led::Button);
                } else {
                    leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
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
                LatchLayer::Alt => storage.query(|s| s.max_div),
                LatchLayer::Third => storage.query(|s| s.min_div),
            };

            if let Some(new_value) = latch.update(fader.get_value(), latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Main => {
                        let val = rescale_12bit_int(
                            new_value,
                            storage.query(|s| s.min_div),
                            storage.query(|s| s.max_div),
                        );
                        div_glob.set(value_to_resolution(val, resolution));
                        storage.modify_and_save(|s| s.fader_saved = new_value);
                    }
                    LatchLayer::Alt => {
                        let min = storage.query(|s| s.min_div);
                        storage.modify_and_save(|s| s.max_div = new_value.max(min));
                        max_glob.set(value_to_resolution(new_value.min(max), resolution));
                    }
                    LatchLayer::Third => {
                        let max = storage.query(|s| s.max_div);
                        storage.modify_and_save(|s| s.min_div = new_value.min(max));
                        min_glob.set(value_to_resolution(new_value.min(max), resolution));
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
                    let (res, mute) = storage.query(|s| (s.fader_saved, s.mute_saved));

                    glob_muted.set(mute);
                    div_glob.set(value_to_resolution(res, resolution));
                    if mute {
                        leds.unset(0, Led::Button);
                        jack.set_low().await;
                        leds.unset(0, Led::Top);
                        leds.unset(0, Led::Bottom);
                    } else {
                        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
                    }
                }

                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    let shift = async {
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
        }
    };

    join5(fut1, fut2, fut3, scene_handler, shift).await;
}
