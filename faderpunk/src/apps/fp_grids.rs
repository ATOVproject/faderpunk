// Copyright (C) 2012 Emilie Gillet.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.
//
// Based on the original Grids by Emilie Gillet.
//
// Acknowledgements
//
// * Original Concept & Code: Emilie Gillet (Mutable Instruments). The original Eurorack module source code can be found [here](https://github.com/pichenettes/eurorack/tree/master/grids).
// * Faderpunk Port: Richard Smith (GitHub: rjsmith)
// * Special acknowledgement: [Disting NT Port](https://github.com/thorinside/nt_grids/tree/main) by Neal Sanche (GitHub: Thorinside)
//
// Layout for Easter Egg DnB mode
// ==============================
//
//! | Control | Function | + Shift | + Fn
//! |---------|----------|---------|------|
//! | Jack 1  | Kick Out | N/A     | N/A  |
//! | Fader 1  | Probability Kick | N/A  | N/A  |
//! | LED 1 Top | Gate output | Gate output | N/A
//! | LED 1 Bottom | Density Kick | N/A | N/A
//! | Fn 1    | Mute Trigger 1 | Vary DnB Pattern | N/A |
//! | Jack 2  | Snare Out | N/A     | N/A  |
//! | Fader 2  | Probability 2 | N/A  | N/A  |
//! | LED 2 Top | Gate output | Gate output | N/A
//! | LED 2 Bottom | Density 2 | N/A | N/A
//! | Fn 2    | Mute Trigger 2 | Restore DnB Pattern | N/A |
//! | Jack 3  | Hi-Hats Out | N/A     | N/A  |
//! | Fader 3  | DnB Pattern (1-12) | N/A  | N/A  |
//! | LED 3 Top | Gate output | Gate output | N/A
//! | LED 3 Bottom | DnB Pattern (1-12) | N/A | N/A
//! | Fn 3    | Mute Trigger 3 | N/A | N/A |
//! | Jack 4  | Ghost Out | N/A     | N/A  |
//! | Fader 4  | Probability Ghost | N/A  | N/A  |
//! | LED 4 Top | Ghost output | Ghost output | N/A
//! | LED 4 Bottom | Probability Ghost | N/A | N/A
//! | Fn 4    | Mute Ghost | Mode (Light Blue=Drums, Pink = Euclidean, Sand = DnB) | N/A |
//!
//! DnB mode ignores the app's clock division, instead set according to the selected DnB pattern
//! MIDI Note for Ghost Snare = Midi Note for Trigegr 3 + one semitone
//!
use embassy_futures::{
    join::{join, join5},
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use enum_ordinalize::Ordinalize;
use heapless::Vec;

use libfp::{
    ext::FromValue,
    fp_grids_lib::{
        OutputMode, PatternGenerator, PatternModeSettings, SequencerState, DNB_NUM_PATTERNS,
        K_NUM_PARTS,
    },
    latch::LatchLayer,
    utils::scale_bits_12_8,
    AppIcon, Brightness, ClockDivision, Color, Config, Curve, MidiChannel, MidiNote, MidiOut,
    Param, Value, APP_MAX_PARAMS,
};

use serde::{Deserialize, Serialize};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Global, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 4; // Number of used faderpunk channels
pub const PARAMS: usize = 9; // NUmber of app configuration parameters

const DIV_SIXTEENTH_NOTE_COLOR: Color = Color::Yellow;

// App configuration visible to the configurator
pub static CONFIG: Config<PARAMS> = Config::new(
    "FP Grids",
    "Topographic drum sequencer port of Mutable Instruments Grids, synced to internal clock",
    Color::SkyBlue,
    AppIcon::Euclid,
)
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiNote {
    name: "MIDI Note 1",
})
.add_param(Param::MidiNote {
    name: "MIDI Note 2",
})
.add_param(Param::MidiNote {
    name: "MIDI Note 3",
})
.add_param(Param::i32 {
    name: "MIDI Velocity",
    min: 1,
    max: 127,
})
.add_param(Param::i32 {
    name: "MIDI Accent Vel",
    min: 1,
    max: 127,
})
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
    midi_out: MidiOut,
    note1: MidiNote,
    note2: MidiNote,
    note3: MidiNote,
    velocity: i32,
    accent: i32,
    gatel: i32,
    color: Color,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_channel: MidiChannel::default(),
            midi_out: MidiOut::default(),
            note1: MidiNote::from(36),
            note2: MidiNote::from(37),
            note3: MidiNote::from(38),
            velocity: 100,
            accent: 127,
            gatel: 50,
            color: Color::Orange,
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            midi_channel: MidiChannel::from_value(values[0]),
            note1: MidiNote::from_value(values[1]),
            note2: MidiNote::from_value(values[2]),
            note3: MidiNote::from_value(values[3]),
            velocity: i32::from_value(values[4]),
            accent: i32::from_value(values[5]),
            gatel: i32::from_value(values[6]),
            color: Color::from_value(values[7]),
            midi_out: MidiOut::from_value(values[8]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.note1.into()).unwrap();
        vec.push(self.note2.into()).unwrap();
        vec.push(self.note3.into()).unwrap();
        vec.push(self.velocity.into()).unwrap();
        vec.push(self.accent.into()).unwrap();
        vec.push(self.gatel.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    fader_saved: [u16; K_NUM_PARTS + 1],
    shift_fader_saved: [u16; K_NUM_PARTS],
    #[serde(default)]
    euclidean_offset_saved: [u8; K_NUM_PARTS],
    div_fader_saved: u16, // 0 - 4095 range, maps to index into 'resolution' clock div array (same as euclid.rs)
    mute_saved: [bool; K_NUM_PARTS + 1], // 3 triggers + accent
    drum_mode: u8,        // 0 = Drums Mode, 1 = Euclidean mode, 2 = DnB Mode
    generator_state: SequencerState, // Internal generator state, use to restore after app re-spawn
    note_on: [bool; K_NUM_PARTS],
    accent_on: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            fader_saved: [2047, 2047, 2047, 0 /* zero chaos */],
            shift_fader_saved: [2047; K_NUM_PARTS],
            euclidean_offset_saved: [0; K_NUM_PARTS],
            div_fader_saved: 3000,
            mute_saved: [false; K_NUM_PARTS + 1],
            drum_mode: 0,
            generator_state: SequencerState::default(),
            note_on: [false; K_NUM_PARTS],
            accent_on: false,
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
    let die = app.use_die();
    let faders = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let (
        midi_out,
        midi_channel,
        gatel,
        note1,
        note2,
        note3,
        velocityi32,
        accent_velocityi32,
        led_color,
    ) = params.query(|p| {
        (
            p.midi_out,
            p.midi_channel,
            p.gatel,
            p.note1,
            p.note2,
            p.note3,
            p.velocity,
            p.accent,
            p.color,
        )
    });
    let alt_led_color = if led_color == Color::Blue {
        Color::Lime
    } else {
        Color::Blue
    };

    let drums_btn_color = Color::LightBlue;
    let euclidean_btn_color = Color::Pink;
    let dnb_btn_color = Color::Sand;

    let midi_velocity = ((velocityi32.abs().clamp(1, 127) as u32 * 4095) / 127) as u16;
    let accent_velocity = ((accent_velocityi32.abs().clamp(1, 127) as u32 * 4095) / 127) as u16;

    let midi = app.use_midi_output(midi_out, midi_channel);
    let notes = [note1, note2, note3];
    let jack = [
        app.make_gate_jack(0, 4095).await,
        app.make_gate_jack(1, 4095).await,
        app.make_gate_jack(2, 4095).await,
        app.make_gate_jack(3, 4095).await,
    ];
    let resolution = [384, 192, 96, 48, 24, 16, 12, 8, 6, 4, 3, 2];
    let div_glob = app.make_global(6); // = 1/16th note
    let glob_latch_layer = app.make_global(LatchLayer::Main);
    // use globs to track pattern generator parameter values, transformed from fader values
    let drums_density_glob = app.make_global([127u8; K_NUM_PARTS]); // 0 - 255 fill density
    let drums_map_x_glob = app.make_global(127u8); // 0 - 255 Map X
    let drums_map_y_glob = app.make_global(127u8); // 0 - 255 Map Y
    let euclidean_length_glob = app.make_global([16u8; K_NUM_PARTS]); // 1 - 16 steps
    let euclidean_fill_glob = app.make_global([8u8; K_NUM_PARTS]); // 0 - 16 pulses (derived from length)
    let euclidean_offset_glob = app.make_global([0u8; K_NUM_PARTS]);
    let chaos_glob = app.make_global(0u8);
    let note_on_glob = app.make_global([false; K_NUM_PARTS]);
    let accent_on_glob = app.make_global(false);
    let output_mode_glob = app.make_global(OutputMode::OutputModeDrums);
    let curve = Curve::Linear;
    let dnb_pattern_glob = app.make_global(0); // Patterns 0 - 11
    let dnb_vary_pattern_glob = app.make_global(false); // Signals if DnB pattern should be varied
    let dnb_reset_pattern_glob = app.make_global(false); // Signals if DnB pattern should be reset to base state

    refresh_state_from_storage(
        storage,
        leds,
        led_color,
        alt_led_color,
        resolution,
        &RefreshStateFromStorageContext {
            div_glob: &div_glob,
            glob_latch_layer: &glob_latch_layer,
            drums_density_glob: &drums_density_glob,
            drums_map_x_glob: &drums_map_x_glob,
            drums_map_y_glob: &drums_map_y_glob,
            euclidean_length_glob: &euclidean_length_glob,
            euclidean_fill_glob: &euclidean_fill_glob,
            euclidean_offset_glob: &euclidean_offset_glob,
            chaos_glob: &chaos_glob,
            output_mode_glob: &output_mode_glob,
            dnb_pattern_glob: &dnb_pattern_glob,
        },
    );

    reset_all_outputs(midi, leds, notes, &jack, &note_on_glob, &accent_on_glob).await;

    let main_loop = async {
        let mut clock = app.use_clock();
        let ticks = clock.get_ticker();

        let mut output_mode = output_mode_glob.get();
        let mut dnb_pattern = dnb_pattern_glob.get();
        let mut tick_origin = ticks() as u32;
        let ghost_note = notes[2].clone().transpose(1);
        let ghost_velocity = (midi_velocity - (midi_velocity / 4)).clamp(1, 127);

        let mut generator = PatternGenerator::default();
        generator.set_seed(die.roll());
        generator.set_output_mode(output_mode);
        generator.set_global_chaos(true);
        update_generator_from_parameters(
            &mut generator,
            &GeneratorUpdateContext {
                drums_density_glob: &drums_density_glob,
                drums_map_x_glob: &drums_map_x_glob,
                drums_map_y_glob: &drums_map_y_glob,
                euclidean_length_glob: &euclidean_length_glob,
                euclidean_fill_glob: &euclidean_fill_glob,
                euclidean_offset_glob: &euclidean_offset_glob,
                chaos_glob: &chaos_glob,
                dnb_pattern_glob: &dnb_pattern_glob,
            },
        );
        let (gen_state_, restored_note_on_, restored_accent_on_) =
            storage.query(|s| (s.generator_state, s.note_on, s.accent_on));
        // Note: sequence_step and euclidean_step from gen_state_ are not used after restore —
        // tick() will recompute them from the absolute tick count on the first clock event.
        // pulse_ and DnB pattern state are the meaningful fields being restored here.
        generator.restore(gen_state_);
        // Decide if need to send MIDI note off events after a re-spawn, assume MIDI notes have not changed since last restore
        for (part, note) in notes.iter().enumerate().take(K_NUM_PARTS) {
            if restored_note_on_[part] {
                midi.send_note_off(*note).await;
            }
        }
        if output_mode == OutputMode::OutputModeDnB && restored_accent_on_ {
            midi.send_note_off(ghost_note).await;
        }

        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset => {
                    // defmt::info!("[{}] Clock reset!", ticks());
                    tick_origin = ticks() as u32;
                    output_mode = output_mode_glob.get();
                    reset_all_outputs(midi, leds, notes, &jack, &note_on_glob, &accent_on_glob)
                        .await;

                    generator.set_seed(die.roll());
                    generator.set_output_mode(output_mode);
                    generator.reset();
                    dnb_vary_pattern_glob.set(false);
                    dnb_reset_pattern_glob.set(false);
                }
                ClockEvent::Stop => {
                    // defmt::info!("[{}] Clock stop", ticks());
                    // Prevent hanging notes / gate CVs if clock is stopped
                    reset_all_outputs(midi, leds, notes, &jack, &note_on_glob, &accent_on_glob)
                        .await;
                    dnb_vary_pattern_glob.set(false);
                    dnb_reset_pattern_glob.set(false);
                }
                ClockEvent::Start => {
                    // defmt::info!("[{}] Clock start", ticks());
                    tick_origin = ticks() as u32;
                    generator.reset();
                    // Ensure initial DnB pattern is generated at start of sequence
                    if output_mode == OutputMode::OutputModeDnB {
                        generator.queue_dnb_pattern_change(dnb_pattern_glob.get());
                    }
                }
                // Assume always 24PPQN
                ClockEvent::Tick => {
                    let muted = storage.query(|s| s.mute_saved);
                    let div = match output_mode {
                        OutputMode::OutputModeDrums => 3, // Grids Drum mode fixed to 1/32nd ticks
                        OutputMode::OutputModeEuclidean => div_glob.get(), // Modified Grids Euclidean can use any division, default 1/16th
                        OutputMode::OutputModeDnB => generator.get_dnb_24ppqn_pattern_division(),
                    };

                    let clkn = (ticks() as u32).wrapping_sub(tick_origin);
                    // If we have reached the next sequence step, or on the first step
                    if clkn.is_multiple_of(div) {
                        // If output mode has changed since last step, change generator mode and reset the sequence
                        if output_mode_glob.get() != output_mode {
                            output_mode = output_mode_glob.get();
                            generator.set_output_mode(output_mode);
                            generator.reset();
                        }
                        // If DnB pattern has changed since last step, queue pattern change for start of next pattern sequence
                        if output_mode == OutputMode::OutputModeDnB {
                            if dnb_pattern_glob.get() != dnb_pattern {
                                dnb_pattern = dnb_pattern_glob.get();
                                generator.queue_dnb_pattern_change(dnb_pattern);
                            }
                            if dnb_vary_pattern_glob.get() {
                                dnb_vary_pattern_glob.toggle();
                                generator.generate_dnb_variation();
                            }
                            if dnb_reset_pattern_glob.get() {
                                dnb_reset_pattern_glob.toggle();
                                generator.reset_dnb_pattern_to_base();
                            }
                        }

                        // Advance sequence step derived from absolute tick count / division
                        generator.tick(clkn, div);

                        // Get generator state and handle individual triggers
                        // State byte bits:
                        // 0: Trigger 1
                        // 1: Trigger 2
                        // 2: Trigger 3
                        // 3: Global accent (or Ghost Snare in DnB mode)
                        let state = generator.get_trigger_state();
                        // defmt::info!("[{}] Step: {}, BD {}, SN {}, HH {} ", clkn, generator.get_step(), state & (1 << 0) > 0, state & (1 << 1) > 0, state & (1 << 2) > 0);
                        let is_accent = state & (1 << 3) > 0;
                        let velocity_ = if output_mode == OutputMode::OutputModeDnB || !is_accent {
                            midi_velocity
                        } else {
                            accent_velocity
                        };

                        for (part, note) in notes.iter().enumerate().take(K_NUM_PARTS) {
                            if state & (1 << part) > 0 && !muted[part] {
                                // Trigger fired

                                // Send Note Off first if re-triggering
                                let mut note_on_ = note_on_glob.get();
                                if note_on_[part] {
                                    midi.send_note_off(*note).await;
                                }
                                jack[part].set_high().await;
                                note_on_[part] = true;
                                note_on_glob.set(note_on_);
                                midi.send_note_on(*note, velocity_).await;
                                leds.set(
                                    part,
                                    Led::Top,
                                    led_color,
                                    if is_accent {
                                        Brightness::High
                                    } else {
                                        Brightness::Mid
                                    },
                                );
                            }
                        }

                        // If accent triggered
                        if is_accent & !muted[3] {
                            // Accent fired
                            jack[3].set_high().await;
                            accent_on_glob.set(true);
                            leds.set(3, Led::Top, led_color, Brightness::High);
                            if output_mode == OutputMode::OutputModeDnB {
                                // Send Ghost Snare MIDI out, use next MIDI note up from Trigger 3
                                midi.send_note_on(ghost_note, ghost_velocity).await;
                            }
                        }

                        // Update generator with parameter changes
                        update_generator_from_parameters(
                            &mut generator,
                            &GeneratorUpdateContext {
                                drums_density_glob: &drums_density_glob,
                                drums_map_x_glob: &drums_map_x_glob,
                                drums_map_y_glob: &drums_map_y_glob,
                                euclidean_length_glob: &euclidean_length_glob,
                                euclidean_fill_glob: &euclidean_fill_glob,
                                euclidean_offset_glob: &euclidean_offset_glob,
                                chaos_glob: &chaos_glob,
                                dnb_pattern_glob: &dnb_pattern_glob,
                            },
                        );

                        // Save generator state in case app is re-spawned
                        storage.modify_and_save(|s| {
                            s.generator_state = generator.get_sequencer_state();
                            s.note_on = note_on_glob.get();
                            s.accent_on = accent_on_glob.get();
                        });

                        if glob_latch_layer.get() == LatchLayer::Alt {
                            if matches!(div, 2 | 4 | 8 | 16) {
                                leds.set(3, Led::Bottom, Color::Orange, Brightness::Mid);
                            } else if div == 6 {
                                // Highlight 1/16th note default
                                leds.set(3, Led::Bottom, Color::Yellow, Brightness::High);
                            } else {
                                leds.set(3, Led::Bottom, Color::Blue, Brightness::Mid);
                            }
                        }
                    }

                    // If reached end of gate length between sequence steps
                    if clkn % div == (div * gatel as u32 / 100).clamp(1, div - 1) {
                        let mut note_on_ = note_on_glob.get();
                        for (part, note) in notes.iter().enumerate().take(K_NUM_PARTS) {
                            if note_on_[part] {
                                midi.send_note_off(*note).await;
                                note_on_[part] = false;
                                jack[part].set_low().await;
                            }
                            leds.unset(part, Led::Top);
                        }
                        note_on_glob.set(note_on_);
                        // Accent jack
                        if accent_on_glob.get() {
                            accent_on_glob.set(false);
                            jack[3].set_low().await;
                            leds.unset(3, Led::Top);
                            if output_mode == OutputMode::OutputModeDnB {
                                midi.send_note_off(ghost_note).await;
                            }
                        }
                        if glob_latch_layer.get() == LatchLayer::Alt {
                            leds.set(3, Led::Bottom, Color::Blue, Brightness::Off);
                        }
                    }
                }
            }
        }
    };

    let fader_fut = async {
        let mut latch = [
            app.make_latch(faders.get_value_at(0)),
            app.make_latch(faders.get_value_at(1)),
            app.make_latch(faders.get_value_at(2)),
            app.make_latch(faders.get_value_at(3)),
        ];
        loop {
            let chan = faders.wait_for_any_change().await;
            let latch_layer = glob_latch_layer.get();
            let mut fader_led_value = 0u16;
            let output_mode_ = output_mode_glob.get();
            match output_mode_ {
                OutputMode::OutputModeDrums => {
                    match chan {
                        0 => {
                            let target_value = match latch_layer {
                                LatchLayer::Main => storage.query(|s| s.fader_saved[chan]),
                                LatchLayer::Alt => storage.query(|s| s.shift_fader_saved[chan]),
                                _ => 0,
                            };
                            if let Some(new_value) = latch[chan].update(
                                faders.get_value_at(chan),
                                latch_layer,
                                target_value,
                            ) {
                                match latch_layer {
                                    LatchLayer::Main => {
                                        // Convert fader value 0 .. 4095 12-bit to Drums density 0 .. 255 8 - bit
                                        let mut drums_density = drums_density_glob.get();
                                        drums_density[chan] = scale_bits_12_8(new_value);
                                        drums_density_glob.set(drums_density);
                                        storage
                                            .modify_and_save(|s| s.fader_saved[chan] = new_value);
                                    }
                                    LatchLayer::Alt => {
                                        // Convert fader value 0 .. 4095 12 bit to Drums Map X 0 .. 255
                                        drums_map_x_glob.set(scale_bits_12_8(new_value));
                                        storage.modify_and_save(|s| {
                                            s.shift_fader_saved[chan] = new_value
                                        });
                                    }
                                    _ => {}
                                };
                                fader_led_value = new_value;
                            }
                        }
                        1 => {
                            let target_value = match latch_layer {
                                LatchLayer::Main => storage.query(|s| s.fader_saved[chan]),
                                LatchLayer::Alt => storage.query(|s| s.shift_fader_saved[chan]),
                                _ => 0,
                            };
                            if let Some(new_value) = latch[chan].update(
                                faders.get_value_at(chan),
                                latch_layer,
                                target_value,
                            ) {
                                match latch_layer {
                                    LatchLayer::Main => {
                                        // Convert fader value 0 .. 4095 12-bit to Drums density 0 .. 255 8 - bit
                                        let mut drums_density = drums_density_glob.get();
                                        drums_density[chan] = scale_bits_12_8(new_value);
                                        drums_density_glob.set(drums_density);
                                        storage
                                            .modify_and_save(|s| s.fader_saved[chan] = new_value);
                                    }
                                    LatchLayer::Alt => {
                                        // Convert fader value 0 .. 4095 12 bit to Drums Map X 0 .. 255
                                        drums_map_y_glob.set(scale_bits_12_8(new_value));
                                        storage.modify_and_save(|s| {
                                            s.shift_fader_saved[chan] = new_value
                                        });
                                    }
                                    _ => {}
                                };
                                fader_led_value = new_value;
                            }
                        }
                        2 => {
                            let target_value = match latch_layer {
                                LatchLayer::Main => storage.query(|s| s.fader_saved[chan]),
                                _ => 0,
                            };
                            if let Some(new_value) = latch[chan].update(
                                faders.get_value_at(chan),
                                latch_layer,
                                target_value,
                            ) {
                                match latch_layer {
                                    LatchLayer::Main => {
                                        // Convert fader value 0 .. 4095 12-bit to Drums density 0 .. 255 8 - bit
                                        let mut drums_density = drums_density_glob.get();
                                        drums_density[chan] = scale_bits_12_8(new_value);
                                        drums_density_glob.set(drums_density);
                                        storage
                                            .modify_and_save(|s| s.fader_saved[chan] = new_value);
                                    }
                                    _ => {
                                        // NB: used single-match for consistency with code in other channels
                                    }
                                };
                                fader_led_value = new_value;
                            }
                        }
                        3 => {
                            let target_value = match latch_layer {
                                LatchLayer::Main => storage.query(|s| s.fader_saved[chan]),
                                LatchLayer::Alt => storage.query(|s| s.div_fader_saved),
                                _ => 0,
                            };
                            if let Some(new_value) = latch[chan].update(
                                faders.get_value_at(chan),
                                latch_layer,
                                target_value,
                            ) {
                                match latch_layer {
                                    LatchLayer::Main => {
                                        // Convert fader value 0 .. 4095 to chaos 0 .. 255
                                        chaos_glob.set(scale_bits_12_8(new_value));
                                        storage
                                            .modify_and_save(|s| s.fader_saved[chan] = new_value);
                                        fader_led_value = new_value;
                                    }
                                    LatchLayer::Alt => {
                                        // Convert fader value 0 .. 4095 to "resolution" lookup array index
                                        div_glob
                                            .set(resolution[curve.at(new_value) as usize / 345]);
                                        storage.modify_and_save(|s| s.div_fader_saved = new_value);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    };
                }
                OutputMode::OutputModeEuclidean => {
                    let target_value = match latch_layer {
                        LatchLayer::Main => storage.query(|s| s.fader_saved[chan]),
                        LatchLayer::Alt => {
                            if chan < K_NUM_PARTS {
                                storage.query(|s| s.shift_fader_saved[chan])
                            } else {
                                storage.query(|s| s.div_fader_saved)
                            }
                        }
                        _ => 0,
                    };
                    if let Some(new_value) =
                        latch[chan].update(faders.get_value_at(chan), latch_layer, target_value)
                    {
                        if chan < K_NUM_PARTS {
                            match latch_layer {
                                LatchLayer::Main => {
                                    // Convert fader value 0..4095 to Euclidean fill parameter in range 0..length,
                                    // where length is derived from the channel's ALT fader.
                                    let mut fill_ = euclidean_fill_glob.get();
                                    let length_value = storage.query(|s| s.shift_fader_saved[chan]);
                                    let length = euclidean_length_from_fader(length_value);
                                    fill_[chan] = euclidean_fill_from_fader(new_value, length);
                                    euclidean_fill_glob.set(fill_);
                                    storage.modify_and_save(|s| s.fader_saved[chan] = new_value);
                                    fader_led_value = new_value;
                                }
                                LatchLayer::Alt => {
                                    // Convert fader value 0..4095 to Euclidean length parameter in range 1..16 (steps)
                                    let mut length_ = euclidean_length_glob.get();
                                    let mapped_length = euclidean_length_from_fader(new_value);
                                    length_[chan] = mapped_length;
                                    euclidean_length_glob.set(length_);

                                    let mut offset_ = euclidean_offset_glob.get();
                                    offset_[chan] %= mapped_length;
                                    euclidean_offset_glob.set(offset_);

                                    let mut fill_ = euclidean_fill_glob.get();
                                    let stored_fill_value = storage.query(|s| s.fader_saved[chan]);
                                    fill_[chan] =
                                        euclidean_fill_from_fader(stored_fill_value, mapped_length);
                                    euclidean_fill_glob.set(fill_);
                                    storage.modify_and_save(|s| {
                                        s.shift_fader_saved[chan] = new_value;
                                        s.euclidean_offset_saved[chan] = offset_[chan];
                                    });
                                    fader_led_value = new_value;
                                }
                                _ => {}
                            }
                        } else if chan == K_NUM_PARTS {
                            match latch_layer {
                                LatchLayer::Main => {
                                    // Convert fader value 0 .. 4095 to chaos 0 .. 255
                                    chaos_glob.set(scale_bits_12_8(new_value));
                                    storage.modify_and_save(|s| s.fader_saved[chan] = new_value);
                                    fader_led_value = new_value;
                                }
                                LatchLayer::Alt => {
                                    // Convert fader value 0 .. 4095 to "resolution" lookup array index
                                    div_glob.set(resolution[new_value as usize / 345]);
                                    storage.modify_and_save(|s| s.div_fader_saved = new_value);
                                    fader_led_value = new_value;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                OutputMode::OutputModeDnB => {
                    match chan {
                        0 | 1 => {
                            let target_value = match latch_layer {
                                LatchLayer::Main => storage.query(|s| s.fader_saved[chan]),
                                LatchLayer::Alt => storage.query(|s| s.shift_fader_saved[chan]),
                                _ => 0,
                            };
                            if let Some(new_value) = latch[chan].update(
                                faders.get_value_at(chan),
                                latch_layer,
                                target_value,
                            ) {
                                if latch_layer == LatchLayer::Main {
                                    // Convert fader value 0 .. 4095 12-bit to BD/Snare density 0 .. 255 8 - bit
                                    let mut drums_density = drums_density_glob.get();
                                    drums_density[chan] = scale_bits_12_8(new_value);
                                    drums_density_glob.set(drums_density);
                                    storage.modify_and_save(|s| s.fader_saved[chan] = new_value);
                                };
                                fader_led_value = new_value;
                            }
                        }
                        2 => {
                            let target_value = match latch_layer {
                                LatchLayer::Main => storage.query(|s| s.fader_saved[chan]),
                                _ => 0,
                            };
                            if let Some(new_value) = latch[chan].update(
                                faders.get_value_at(chan),
                                latch_layer,
                                target_value,
                            ) {
                                match latch_layer {
                                    LatchLayer::Main => {
                                        // Convert fader value 0 .. 4095 12-bit to DnB Pattern Id
                                        dnb_pattern_glob
                                            .set(scale_bits_12_8(new_value) / DNB_NUM_PATTERNS);
                                        storage
                                            .modify_and_save(|s| s.fader_saved[chan] = new_value);
                                    }
                                    _ => {
                                        // NB: used single-match for consistency with code in other channels
                                    }
                                };
                                fader_led_value = new_value;
                            }
                        }
                        3 => {
                            let target_value = match latch_layer {
                                LatchLayer::Main => storage.query(|s| s.fader_saved[chan]),
                                _ => 0,
                            };
                            if let Some(new_value) = latch[chan].update(
                                faders.get_value_at(chan),
                                latch_layer,
                                target_value,
                            ) {
                                if latch_layer == LatchLayer::Main {
                                    // Convert fader value 0 .. 4095 12-bit to Ghost Snare density 0 .. 255 8 - bit
                                    let mut drums_density = drums_density_glob.get();
                                    drums_density[2] = scale_bits_12_8(new_value);
                                    drums_density_glob.set(drums_density);
                                    storage.modify_and_save(|s| s.fader_saved[chan] = new_value);
                                }
                            }
                        }
                        _ => {}
                    };
                }
            };

            // Update fader-derived Leds
            match output_mode_ {
                OutputMode::OutputModeDrums => {
                    match latch_layer {
                        LatchLayer::Main => {
                            leds.set(
                                chan,
                                Led::Bottom,
                                led_color,
                                Brightness::Custom(scale_bits_12_8(fader_led_value)),
                            );
                        }
                        LatchLayer::Alt => {
                            if chan != 3 {
                                leds.set(
                                    chan,
                                    Led::Bottom,
                                    alt_led_color,
                                    Brightness::Custom(scale_bits_12_8(fader_led_value)),
                                );
                            }
                        }
                        _ => {}
                    };
                }
                OutputMode::OutputModeEuclidean => match latch_layer {
                    LatchLayer::Main => {
                        leds.set(
                            chan,
                            Led::Bottom,
                            led_color,
                            Brightness::Custom(scale_bits_12_8(fader_led_value)),
                        );
                    }
                    LatchLayer::Alt => {
                        if chan == 3 && div_glob.get() == 6 {
                            leds.set(3, Led::Bottom, DIV_SIXTEENTH_NOTE_COLOR, Brightness::High);
                        } else {
                            leds.set(
                                chan,
                                Led::Bottom,
                                alt_led_color,
                                Brightness::Custom(scale_bits_12_8(fader_led_value)),
                            );
                        }
                    }
                    _ => {}
                },
                OutputMode::OutputModeDnB => {
                    match latch_layer {
                        LatchLayer::Main => {
                            leds.set(
                                chan,
                                Led::Bottom,
                                if chan == 2 {
                                    // Distinguish DnB pattern selector fader value from density faders
                                    alt_led_color
                                } else {
                                    led_color
                                },
                                Brightness::Custom(scale_bits_12_8(fader_led_value)),
                            );
                        }
                        LatchLayer::Alt => {
                            leds.unset(chan, Led::Bottom);
                        }
                        _ => {}
                    };
                }
            }
        }
    };

    let buttons_fut = async {
        loop {
            let (part, shift) = buttons.wait_for_any_down().await;
            if !shift {
                if part < K_NUM_PARTS {
                    // Handle trigger muting
                    let mut muted_ = storage.query(|s| s.mute_saved);
                    muted_[part] = !muted_[part];
                    if note_on_glob.get()[part] {
                        let mut note_on_ = note_on_glob.get();
                        if note_on_[part] {
                            midi.send_note_off(notes[part]).await;
                            note_on_[part] = false;
                            note_on_glob.set(note_on_);
                            jack[part].set_low().await;
                        }
                        leds.unset(part, Led::Top);
                    }
                    storage.modify_and_save(|s| {
                        s.mute_saved = muted_;
                        s.mute_saved
                    });

                    // Show muted trigger button state
                    if muted_[part] {
                        leds.unset(part, Led::Button);
                    } else {
                        leds.set(part, Led::Button, led_color, Brightness::High);
                    }
                } else if part == K_NUM_PARTS {
                    // accent mute
                    let mut muted_ = storage.query(|s| s.mute_saved);
                    muted_[part] = !muted_[part];
                    accent_on_glob.set(muted_[part]);
                    if muted_[part] {
                        jack[part].set_low().await;
                        leds.unset(part, Led::Top);
                        leds.unset(part, Led::Button);
                    }
                    storage.modify_and_save(|s| {
                        s.mute_saved = muted_;
                        s.mute_saved
                    });

                    // Show muted trigger button state
                    if muted_[part] {
                        leds.unset(part, Led::Button);
                    } else {
                        leds.set(part, Led::Button, led_color, Brightness::High);
                    }
                }
            } else if part < K_NUM_PARTS
                && output_mode_glob.get() == OutputMode::OutputModeEuclidean
            {
                // Update Euclidean parameters from fader movement
                let mut offset_ = euclidean_offset_glob.get();
                let length = euclidean_length_glob.get()[part].max(1);
                offset_[part] = (offset_[part] + 1) % length;
                euclidean_offset_glob.set(offset_);
                storage.modify_and_save(|s| s.euclidean_offset_saved[part] = offset_[part]);
            } else if part == 0 && output_mode_glob.get() == OutputMode::OutputModeDnB {
                // DnB pattern will be varied on next sequencer step
                dnb_vary_pattern_glob.set(true);
            } else if part == 1 && output_mode_glob.get() == OutputMode::OutputModeDnB {
                // DnB pattern will be reset to base pattern on next sequencer step
                dnb_reset_pattern_glob.set(true);
            } else if part == K_NUM_PARTS {
                // shift + output mode toggle
                let drum_mode_ = match storage.modify_and_save(|s| {
                    s.drum_mode = (s.drum_mode + 1) % 3; // Increment and fold to range 0, 1, 2
                    s.drum_mode
                }) {
                    0 => OutputMode::OutputModeDrums,
                    1 => OutputMode::OutputModeEuclidean,
                    2 => OutputMode::OutputModeDnB,
                    _ => OutputMode::OutputModeDrums,
                };
                output_mode_glob.set(drum_mode_);
                if glob_latch_layer.get() == LatchLayer::Alt {
                    for part in 0..K_NUM_PARTS {
                        leds.unset(part, Led::Button);
                    }
                    match drum_mode_ {
                        OutputMode::OutputModeDrums => {
                            leds.set(3, Led::Button, drums_btn_color, Brightness::High);
                        }
                        OutputMode::OutputModeEuclidean => {
                            for part in 0..K_NUM_PARTS {
                                leds.set(part, Led::Button, euclidean_btn_color, Brightness::High);
                            }
                            leds.set(3, Led::Button, euclidean_btn_color, Brightness::High);
                        }
                        OutputMode::OutputModeDnB => {
                            leds.set(0, Led::Button, dnb_btn_color, Brightness::High);
                            leds.set(1, Led::Button, dnb_btn_color, Brightness::High);
                            leds.set(3, Led::Button, dnb_btn_color, Brightness::High);
                        }
                    }
                } else {
                    let mutes = storage.query(|s| s.mute_saved);
                    for (part, mute_) in mutes.iter().enumerate().take(K_NUM_PARTS + 1) {
                        if *mute_ {
                            leds.unset(part, Led::Button);
                        } else {
                            leds.set(part, Led::Button, led_color, Brightness::High);
                        }
                    }
                }
            }
        }
    };

    const LATCH_LAYER_DETECTION_MILLIS: u64 = 50;
    let shift_fut = async {
        loop {
            // latching on pressing and depressing shift and channel 0 button
            app.delay_millis(LATCH_LAYER_DETECTION_MILLIS).await;

            let latch_active_layer = if buttons.is_shift_pressed() {
                LatchLayer::Alt
            } else {
                LatchLayer::Main
            };
            if latch_active_layer != glob_latch_layer.get() {
                glob_latch_layer.set(latch_active_layer);
                update_fader_leds(
                    storage,
                    leds,
                    led_color,
                    alt_led_color,
                    output_mode_glob.get(),
                    latch_active_layer,
                    div_glob.get(),
                );

                // Update Button LEDs
                if latch_active_layer == LatchLayer::Main {
                    let mutes = storage.query(|s| s.mute_saved);
                    for (part, mute_) in mutes.iter().enumerate().take(K_NUM_PARTS + 1) {
                        if *mute_ {
                            leds.unset(part, Led::Button);
                        } else {
                            leds.set(part, Led::Button, led_color, Brightness::High);
                        }
                    }
                } else if latch_active_layer == LatchLayer::Alt {
                    match output_mode_glob.get() {
                        OutputMode::OutputModeDrums => {
                            for part in 0..K_NUM_PARTS {
                                leds.unset(part, Led::Button);
                            }
                            leds.set(3, Led::Button, drums_btn_color, Brightness::High);
                        }
                        OutputMode::OutputModeEuclidean => {
                            for part in 0..K_NUM_PARTS {
                                leds.set(part, Led::Button, euclidean_btn_color, Brightness::High);
                            }
                            leds.set(3, Led::Button, euclidean_btn_color, Brightness::High);
                        }
                        OutputMode::OutputModeDnB => {
                            leds.set(0, Led::Button, dnb_btn_color, Brightness::High);
                            leds.set(1, Led::Button, dnb_btn_color, Brightness::High);
                            leds.unset(2, Led::Button);
                            leds.set(3, Led::Button, dnb_btn_color, Brightness::High);
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
                    refresh_state_from_storage(
                        storage,
                        leds,
                        led_color,
                        alt_led_color,
                        resolution,
                        &RefreshStateFromStorageContext {
                            div_glob: &div_glob,
                            glob_latch_layer: &glob_latch_layer,
                            drums_density_glob: &drums_density_glob,
                            drums_map_x_glob: &drums_map_x_glob,
                            drums_map_y_glob: &drums_map_y_glob,
                            euclidean_length_glob: &euclidean_length_glob,
                            euclidean_fill_glob: &euclidean_fill_glob,
                            euclidean_offset_glob: &euclidean_offset_glob,
                            chaos_glob: &chaos_glob,
                            output_mode_glob: &output_mode_glob,
                            dnb_pattern_glob: &dnb_pattern_glob,
                        },
                    );
                    reset_all_outputs(midi, leds, notes, &jack, &note_on_glob, &accent_on_glob)
                        .await;
                }

                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    join5(main_loop, fader_fut, buttons_fut, shift_fut, scene_handler).await;
}

async fn reset_all_outputs(
    midi: crate::app::MidiOutput,
    leds: crate::app::Leds<4>,
    notes: [MidiNote; 3],
    jack: &[crate::app::GateJack; 4],
    note_on_glob: &Global<[bool; 3]>,
    accent_on_glob: &Global<bool>,
) {
    let note_on_ = note_on_glob.get();
    for part in 0..K_NUM_PARTS {
        if note_on_[part] {
            join(
                // Only send a MIDI note off if we think we have previously sent a note on
                midi.send_note_off(notes[part]),
                jack[part].set_low(),
            )
            .await;
        } else {
            jack[part].set_low().await;
        }
        leds.unset(part, Led::Top);
    }
    note_on_glob.set([false; K_NUM_PARTS]);
    jack[3].set_low().await;
    accent_on_glob.set(false);
    leds.unset(3, Led::Top);
}

struct RefreshStateFromStorageContext<'a> {
    div_glob: &'a Global<u32>,
    glob_latch_layer: &'a Global<LatchLayer>,
    drums_density_glob: &'a Global<[u8; 3]>,
    drums_map_x_glob: &'a Global<u8>,
    drums_map_y_glob: &'a Global<u8>,
    euclidean_length_glob: &'a Global<[u8; 3]>,
    euclidean_fill_glob: &'a Global<[u8; 3]>,
    euclidean_offset_glob: &'a Global<[u8; 3]>,
    chaos_glob: &'a Global<u8>,
    output_mode_glob: &'a Global<OutputMode>,
    dnb_pattern_glob: &'a Global<u8>,
}

/// Update in-memory globals from scene-stored data
fn refresh_state_from_storage(
    storage: &ManagedStorage<Storage>,
    leds: crate::app::Leds<4>,
    led_color: Color,
    alt_led_color: Color,
    resolution: [u32; 12],
    globs: &RefreshStateFromStorageContext,
) {
    let (drum_mode_, faders_, shift_faders_, euclidean_offsets_, div_saved_) = storage.query(|s| {
        (
            s.drum_mode,
            s.fader_saved,
            s.shift_fader_saved,
            s.euclidean_offset_saved,
            s.div_fader_saved,
        )
    });
    let output_mode_ = match drum_mode_ {
        0 => OutputMode::OutputModeDrums,
        1 => OutputMode::OutputModeEuclidean,
        2 => OutputMode::OutputModeDnB,
        _ => OutputMode::OutputModeDrums,
    };

    globs.output_mode_glob.set(output_mode_);
    match output_mode_ {
        OutputMode::OutputModeDrums => {
            let drums_density_ = [faders_[0], faders_[1], faders_[2]];
            globs
                .drums_density_glob
                .set(drums_density_.map(scale_bits_12_8));
            globs
                .drums_map_x_glob
                .set(scale_bits_12_8(shift_faders_[0]));
            globs
                .drums_map_y_glob
                .set(scale_bits_12_8(shift_faders_[1]));
            globs.div_glob.set(resolution[div_saved_ as usize / 345]);
            globs.chaos_glob.set(scale_bits_12_8(faders_[3]));
        }
        OutputMode::OutputModeEuclidean => {
            let euclidean_length_ = [shift_faders_[0], shift_faders_[1], shift_faders_[2]];
            let mapped_euclidean_length_ = euclidean_length_.map(euclidean_length_from_fader);
            globs
                .euclidean_offset_glob
                .set(core::array::from_fn(|part| {
                    euclidean_offsets_[part] % mapped_euclidean_length_[part].max(1)
                }));

            let euclidean_fill_ = [faders_[0], faders_[1], faders_[2]];
            globs.euclidean_fill_glob.set(core::array::from_fn(|part| {
                euclidean_fill_from_fader(euclidean_fill_[part], mapped_euclidean_length_[part])
            }));
            globs.euclidean_length_glob.set(mapped_euclidean_length_);
            globs.div_glob.set(resolution[div_saved_ as usize / 345]);
            globs.chaos_glob.set(scale_bits_12_8(faders_[3]));
        }
        OutputMode::OutputModeDnB => {
            let drums_density_ = [faders_[0], faders_[1], faders_[3]];
            globs
                .drums_density_glob
                .set(drums_density_.map(scale_bits_12_8));
            globs.dnb_pattern_glob.set(scale_bits_12_8(faders_[2]));
        }
    }

    // Assume we are always on LatchLayer::Main when switching scenes
    let mutes = storage.query(|s| s.mute_saved);
    for (part, mute_) in mutes.iter().enumerate().take(K_NUM_PARTS + 1) {
        if *mute_ {
            leds.unset(part, Led::Button);
        } else {
            leds.set(part, Led::Button, led_color, Brightness::High);
        }
    }

    // Set up bottom fader - value Leds
    update_fader_leds(
        storage,
        leds,
        led_color,
        alt_led_color,
        output_mode_,
        globs.glob_latch_layer.get(),
        globs.div_glob.get(),
    );
}

/// Update bottom row of Fader Leds from fader values
fn update_fader_leds(
    storage: &ManagedStorage<Storage>,
    leds: crate::app::Leds<4>,
    led_color: Color,
    alt_led_color: Color,
    output_mode: OutputMode,
    latch_active_layer: LatchLayer,
    clock_resolution: u32,
) {
    // Initialise bottom Led fader value Leds
    match output_mode {
        OutputMode::OutputModeDrums => {
            match latch_active_layer {
                LatchLayer::Main => {
                    let faders_ = storage.query(|s| s.fader_saved);
                    for (chan, fader_) in faders_.iter().enumerate().take(K_NUM_PARTS + 1) {
                        leds.set(
                            chan,
                            Led::Bottom,
                            led_color,
                            Brightness::Custom(scale_bits_12_8(*fader_)),
                        );
                    }
                }
                LatchLayer::Alt => {
                    let shift_faders_ = storage.query(|s| s.shift_fader_saved);
                    leds.set(
                        0,
                        Led::Bottom,
                        alt_led_color,
                        Brightness::Custom(scale_bits_12_8(shift_faders_[0])),
                    );
                    leds.set(
                        1,
                        Led::Bottom,
                        alt_led_color,
                        Brightness::Custom(scale_bits_12_8(shift_faders_[1])),
                    );
                    leds.unset(2, Led::Bottom);
                    leds.set(
                        3,
                        Led::Bottom,
                        alt_led_color,
                        Brightness::Custom(scale_bits_12_8(storage.query(|s| s.div_fader_saved))),
                    );
                }
                _ => {}
            };
        }
        OutputMode::OutputModeEuclidean => match latch_active_layer {
            LatchLayer::Main => {
                let faders_ = storage.query(|s| s.fader_saved);
                for (chan, fader_) in faders_.iter().enumerate().take(K_NUM_PARTS + 1) {
                    leds.set(
                        chan,
                        Led::Bottom,
                        led_color,
                        Brightness::Custom(scale_bits_12_8(*fader_)),
                    );
                }
            }
            LatchLayer::Alt => {
                let shift_faders_ = storage.query(|s| s.shift_fader_saved);
                for (chan, shift_fader_) in shift_faders_.iter().enumerate().take(K_NUM_PARTS) {
                    leds.set(
                        chan,
                        Led::Bottom,
                        alt_led_color,
                        Brightness::Custom(scale_bits_12_8(*shift_fader_)),
                    );
                }
            }
            _ => {}
        },
        OutputMode::OutputModeDnB => match latch_active_layer {
            LatchLayer::Main => {
                let faders_ = storage.query(|s| s.fader_saved);
                for (chan, fader_) in faders_.iter().enumerate().take(K_NUM_PARTS + 1) {
                    leds.set(
                        chan,
                        Led::Bottom,
                        if chan == 2 { alt_led_color } else { led_color },
                        Brightness::Custom(scale_bits_12_8(*fader_)),
                    );
                }
            }
            LatchLayer::Alt => {
                let shift_faders_ = storage.query(|s| s.shift_fader_saved);
                for (chan, _) in shift_faders_.iter().enumerate().take(K_NUM_PARTS) {
                    leds.unset(chan, Led::Bottom);
                }
            }
            _ => {}
        },
    }
    // Other Led values
    if output_mode != OutputMode::OutputModeDnB && latch_active_layer == LatchLayer::Alt {
        if clock_resolution == 6 {
            leds.set(3, Led::Bottom, DIV_SIXTEENTH_NOTE_COLOR, Brightness::High)
        } else {
            leds.set(
                3,
                Led::Bottom,
                alt_led_color,
                Brightness::Custom(scale_bits_12_8(
                    (storage.query(|s| s.div_fader_saved) / 345) * 345,
                )),
            );
        }
    }
}

fn euclidean_length_from_fader(value: u16) -> u8 {
    // Same as euclid.rs: fader * 15 / 4095 + 1 → 1..16
    (value as u32 * 15 / 4095) as u8 + 1
}

fn euclidean_fill_from_fader(value: u16, length: u8) -> u8 {
    ((value as u32 * length as u32) / 4095) as u8
}

struct GeneratorUpdateContext<'a> {
    drums_density_glob: &'a Global<[u8; K_NUM_PARTS]>,
    drums_map_x_glob: &'a Global<u8>,
    drums_map_y_glob: &'a Global<u8>,
    euclidean_length_glob: &'a Global<[u8; K_NUM_PARTS]>,
    euclidean_fill_glob: &'a Global<[u8; K_NUM_PARTS]>,
    euclidean_offset_glob: &'a Global<[u8; K_NUM_PARTS]>,
    chaos_glob: &'a Global<u8>,
    dnb_pattern_glob: &'a Global<u8>,
}

/// Update a PatternGenerator options from the app instance's managed parameters
fn update_generator_from_parameters(
    generator: &mut PatternGenerator,
    settings: &GeneratorUpdateContext,
) {
    generator.set_gate_mode(true);
    generator.set_global_chaos(true);
    generator.settings_[OutputMode::OutputModeDrums.ordinal() as usize].options =
        PatternModeSettings::Drums {
            x: settings.drums_map_x_glob.get(),
            y: settings.drums_map_y_glob.get(),
            randomness: settings.chaos_glob.get(),
        };
    generator.settings_[OutputMode::OutputModeDrums.ordinal() as usize].density =
        settings.drums_density_glob.get();
    generator.settings_[OutputMode::OutputModeEuclidean.ordinal() as usize].options =
        PatternModeSettings::Euclidean {
            chaos_amount: settings.chaos_glob.get(),
        };
    generator.settings_[OutputMode::OutputModeEuclidean.ordinal() as usize].density =
        settings.euclidean_fill_glob.get();
    let length: [u8; K_NUM_PARTS] = settings.euclidean_length_glob.get();
    let offset: [u8; K_NUM_PARTS] = settings.euclidean_offset_glob.get();
    for (part, length_) in length.iter().enumerate().take(K_NUM_PARTS) {
        generator.set_length(part, *length_);
        generator.set_offset(part, offset[part]);
    }
    generator.settings_[OutputMode::OutputModeDnB.ordinal() as usize].options =
        PatternModeSettings::DnB {
            pattern: settings.dnb_pattern_glob.get(),
        };
    generator.settings_[OutputMode::OutputModeDnB.ordinal() as usize].density =
        settings.drums_density_glob.get();
}
