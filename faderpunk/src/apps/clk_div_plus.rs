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
    utils::{
        attenuate_bipolar, rescale_12bit_int, resolution_for_mode, resolution_with_input_offset,
        split_unsigned_value, value_to_resolution,
    },
    AppIcon, Brightness, ClockDivision, Color, Config, MidiChannel, MidiNote, MidiOut, Param,
    Range, Value, APP_MAX_PARAMS,
};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 6;

const LED_BRIGHTNESS: Brightness = Brightness::Mid;
const EXT_CLOCK_HIGH_THRESHOLD: u16 = 2458;
const EXT_CLOCK_LOW_THRESHOLD: u16 = 2200;
const EXT_CLOCK_COOLDOWN_MS: u8 = 4;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Clock Divider+",
    "Clock divider with assignable CV input",
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
    in_att: u16,
    in_mute: bool,
    dest: usize,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            fader_saved: 3000,
            mute_saved: false,
            max_div: 4095,
            min_div: 0,
            in_att: 4095,
            in_mute: false,
            dest: 0, // 0 => division, 1 => ext clock
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

    let input = app.make_in_jack(0, Range::_Neg5_5V).await;
    let jack = app.make_gate_jack(1, 4095).await;

    let glob_muted = app.make_global(false);
    let div_glob = app.make_global(6);
    let max_glob = app.make_global(6);
    let min_glob = app.make_global(6);
    let ext_clkn_glob = app.make_global(0_u32);
    let note_on_glob = app.make_global(false);
    let in_val_glob = app.make_global(2047);
    let glob_latch_layer = app.make_global(LatchLayer::Main);

    let resolution = resolution_for_mode(division_mode);

    let (res, mute, min, max) =
        storage.query(|s| (s.fader_saved, s.mute_saved, s.min_div, s.max_div));

    min_glob.set(value_to_resolution(min, resolution));
    max_glob.set(value_to_resolution(max, resolution));

    glob_muted.set(mute);
    div_glob.set(value_to_resolution(res, resolution));

    if mute {
        leds.unset(1, Led::Button);
        leds.unset(1, Led::Top);
        leds.unset(1, Led::Bottom);
        jack.set_low().await;
    } else {
        leds.set(1, Led::Button, led_color, LED_BRIGHTNESS);
    }

    if storage.query(|s| s.in_mute) {
        leds.unset(0, Led::Button);
    } else {
        leds.set(0, Led::Button, led_color, Brightness::Mid);
    }

    let clock_handler = async {
        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset => {
                    ext_clkn_glob.set(0);

                    if note_on_glob.get() {
                        midi.send_note_off(note).await;
                        note_on_glob.set(false);
                    }

                    jack.set_low().await;
                    leds.set(1, Led::Top, led_color, Brightness::Off);
                    if glob_latch_layer.get() == LatchLayer::Main {
                        leds.set(1, Led::Bottom, led_color, Brightness::Off);
                    }
                }
                ClockEvent::Stop => {
                    if note_on_glob.get() {
                        midi.send_note_off(note).await;
                        note_on_glob.set(false);
                    }

                    jack.set_low().await;
                    leds.set(1, Led::Top, led_color, Brightness::Off);
                    if glob_latch_layer.get() == LatchLayer::Main {
                        leds.set(1, Led::Bottom, led_color, Brightness::Off);
                    }
                }
                ClockEvent::Tick => {
                    if storage.query(|s| s.dest) != 0 {
                        continue;
                    }

                    let clkn = ticks() as u32;
                    let muted = glob_muted.get();
                    let div_u32 = div_glob.get();
                    let gate_step = (div_u32 * gatel / 100).clamp(1, div_u32.saturating_sub(1));
                    let latch_layer = glob_latch_layer.get();

                    if clkn.is_multiple_of(div_u32) && !muted {
                        jack.set_high().await;
                        leds.set(1, Led::Top, led_color, LED_BRIGHTNESS);
                        midi.send_note_on(note, 4095).await;
                        note_on_glob.set(true);
                    }

                    if clkn % div_u32 == gate_step {
                        if note_on_glob.get() {
                            midi.send_note_off(note).await;
                            note_on_glob.set(false);
                        }
                        jack.set_low().await;
                        leds.set(1, Led::Top, led_color, Brightness::Off);
                        if latch_layer == LatchLayer::Main {
                            leds.set(1, Led::Bottom, led_color, Brightness::Off);
                        }
                    }

                    if latch_layer != LatchLayer::Main {
                        if clkn.is_multiple_of(max_glob.get()) {
                            leds.set(1, Led::Top, Color::Red, LED_BRIGHTNESS);
                        }
                        if clkn.is_multiple_of(min_glob.get()) {
                            leds.set(1, Led::Bottom, Color::Red, LED_BRIGHTNESS);
                        }
                    }
                }
                _ => {}
            }
        }
    };

    let button_handler = async {
        loop {
            let (chan, shift) = buttons.wait_for_any_down().await;

            if chan == 0 {
                if !shift {
                    let in_mute = storage.modify_and_save(|s| {
                        s.in_mute = !s.in_mute;
                        s.in_mute
                    });

                    if in_mute {
                        leds.unset(0, Led::Button);
                    } else {
                        leds.set(0, Led::Button, led_color, Brightness::Mid);
                    }
                } else {
                    storage.modify_and_save(|s| {
                        s.dest = (s.dest + 1) % 2;
                    });
                }
            }

            if chan == 1 && shift {
                let muted = glob_muted.toggle();

                storage.modify_and_save(|s| {
                    s.mute_saved = muted;
                });

                if muted {
                    jack.set_low().await;
                    leds.unset(1, Led::Button);
                } else {
                    leds.set(1, Led::Button, led_color, LED_BRIGHTNESS);
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
                    if latch_layer == LatchLayer::Main {
                        storage.modify_and_save(|s| s.in_att = new_value);
                    }
                }
            }

            if chan == 1 {
                let target_value = match latch_layer {
                    LatchLayer::Main => storage.query(|s| s.fader_saved),
                    LatchLayer::Alt => storage.query(|s| s.max_div),
                    LatchLayer::Third => storage.query(|s| s.min_div),
                };

                if let Some(new_value) =
                    latch[chan].update(fader.get_value_at(chan), latch_layer, target_value)
                {
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
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    let (res, mute, min, max, in_mute) = storage
                        .query(|s| (s.fader_saved, s.mute_saved, s.min_div, s.max_div, s.in_mute));

                    glob_muted.set(mute);
                    div_glob.set(value_to_resolution(res, resolution));
                    min_glob.set(value_to_resolution(min, resolution));
                    max_glob.set(value_to_resolution(max, resolution));

                    if mute {
                        leds.unset(1, Led::Button);
                        jack.set_low().await;
                        leds.unset(1, Led::Top);
                        leds.unset(1, Led::Bottom);
                    } else {
                        leds.set(1, Led::Button, led_color, LED_BRIGHTNESS);
                    }

                    if in_mute {
                        leds.unset(0, Led::Button);
                    } else {
                        leds.set(0, Led::Button, led_color, Brightness::Mid);
                    }
                }

                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    let shift_handler = async {
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
        }
    };

    let engine_loop = async {
        let mut prev_dest = storage.query(|s| s.dest);
        let mut ext_input_high = false;
        let mut ext_tick_cooldown = 0_u8;

        loop {
            app.delay_millis(1).await;
            ext_tick_cooldown = ext_tick_cooldown.saturating_sub(1);

            let latch_layer = glob_latch_layer.get();
            let (destination, in_mute, in_att, base_speed, max_div, min_div) = storage.query(|s| {
                (
                    s.dest,
                    s.in_mute,
                    s.in_att,
                    s.fader_saved,
                    s.max_div,
                    s.min_div,
                )
            });
            let muted = glob_muted.get();

            let in_val = if in_mute {
                2047
            } else {
                attenuate_bipolar(input.get_value(), in_att)
            };
            in_val_glob.set(in_val);

            let led_in = split_unsigned_value(in_val);
            leds.set(0, Led::Top, led_color, Brightness::Custom(led_in[0]));
            leds.set(0, Led::Bottom, led_color, Brightness::Custom(led_in[1]));

            if latch_layer == LatchLayer::Main {
                if in_mute {
                    leds.unset(0, Led::Button);
                } else {
                    leds.set(0, Led::Button, led_color, Brightness::Mid);
                }
            }

            if latch_layer == LatchLayer::Alt {
                let dest_color = match destination {
                    0 => Color::Yellow,
                    1 => Color::Pink,
                    _ => Color::Yellow,
                };
                leds.set(0, Led::Button, dest_color, Brightness::Mid);
                leds.set(
                    1,
                    Led::Top,
                    Color::Red,
                    Brightness::Custom((max_div / 16) as u8),
                );
                leds.set(
                    1,
                    Led::Bottom,
                    Color::Red,
                    Brightness::Custom((min_div / 16) as u8),
                );
            }

            if latch_layer == LatchLayer::Third {
                leds.set(
                    1,
                    Led::Top,
                    Color::Green,
                    Brightness::Custom((min_div / 16) as u8),
                );
                leds.set(
                    1,
                    Led::Bottom,
                    Color::Green,
                    Brightness::Custom((max_div / 16) as u8),
                );
            }

            let div = if destination == 0 {
                resolution_with_input_offset(base_speed, in_val, resolution)
            } else {
                value_to_resolution(base_speed, resolution)
            };
            div_glob.set(div);
            let div_u32 = div as u32;
            let gate_step = (div_u32 * gatel / 100).clamp(1, div_u32.saturating_sub(1));

            if destination != prev_dest {
                if note_on_glob.get() {
                    midi.send_note_off(note).await;
                    note_on_glob.set(false);
                }
                jack.set_low().await;

                ext_clkn_glob.set(0);
                prev_dest = destination;
            }

            if destination == 1 {
                let should_tick =
                    should_trigger_ext_tick(in_val, &mut ext_input_high, &mut ext_tick_cooldown);

                if should_tick {
                    let mut ext_clkn = ext_clkn_glob.get();

                    if ext_clkn.is_multiple_of(div_u32) && !muted {
                        jack.set_high().await;
                        leds.set(1, Led::Top, led_color, LED_BRIGHTNESS);
                        midi.send_note_on(note, 4095).await;
                        note_on_glob.set(true);
                    }

                    if ext_clkn % div_u32 == gate_step {
                        if note_on_glob.get() {
                            midi.send_note_off(note).await;
                            note_on_glob.set(false);
                        }
                        jack.set_low().await;
                        leds.set(1, Led::Top, led_color, Brightness::Off);
                        if latch_layer == LatchLayer::Main {
                            leds.set(1, Led::Bottom, led_color, Brightness::Off);
                        }
                    }

                    if latch_layer != LatchLayer::Main {
                        if ext_clkn.is_multiple_of(max_glob.get()) {
                            leds.set(1, Led::Top, Color::Red, LED_BRIGHTNESS);
                        }
                        if ext_clkn.is_multiple_of(min_glob.get()) {
                            leds.set(1, Led::Bottom, Color::Red, LED_BRIGHTNESS);
                        }
                    }

                    ext_clkn = ext_clkn.saturating_add(1);
                    ext_clkn_glob.set(ext_clkn);
                }
            } else {
                ext_input_high = false;
            }

            if muted {
                leds.unset(1, Led::Button);
            } else {
                leds.set(1, Led::Button, led_color, LED_BRIGHTNESS);
            }
        }
    };

    join(
        clock_handler,
        join5(
            button_handler,
            fader_handler,
            scene_handler,
            shift_handler,
            engine_loop,
        ),
    )
    .await;
}

fn should_trigger_ext_tick(in_val: u16, ext_input_high: &mut bool, cooldown: &mut u8) -> bool {
    if !*ext_input_high && in_val >= EXT_CLOCK_HIGH_THRESHOLD {
        *ext_input_high = true;
        if *cooldown == 0 {
            *cooldown = EXT_CLOCK_COOLDOWN_MS;
            return true;
        }
    } else if *ext_input_high && in_val <= EXT_CLOCK_LOW_THRESHOLD {
        *ext_input_high = false;
    }

    false
}

