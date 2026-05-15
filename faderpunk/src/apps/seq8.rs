use embassy_futures::{
    join::{join4, join5},
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use libm::expf;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue, latch::LatchLayer, AppIcon, Brightness, ClockDivision, Color, Config,
    MidiChannel, MidiNote, MidiOut, Param, Range, Value, APP_MAX_PARAMS,
};

use crate::app::{
    App, AppParams, AppStorage, Arr, ClockEvent, Global, Led, ManagedStorage, ParamStore,
    SceneEvent,
};

pub const CHANNELS: usize = 8;
pub const PARAMS: usize = 5;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Sequencer",
    "4 x 16 step CV/gate sequencers",
    Color::Yellow,
    AppIcon::Sequence,
)
.add_param(Param::MidiChannel {
    name: "MIDI Channel 1",
})
.add_param(Param::MidiChannel {
    name: "MIDI Channel 2",
})
.add_param(Param::MidiChannel {
    name: "MIDI Channel 3",
})
.add_param(Param::MidiChannel {
    name: "MIDI Channel 4",
})
.add_param(Param::MidiOut);

pub struct Params {
    midi_channel1: MidiChannel,
    midi_channel2: MidiChannel,
    midi_channel3: MidiChannel,
    midi_channel4: MidiChannel,
    midi_out: MidiOut,
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            midi_channel1: MidiChannel::from_value(values[0]),
            midi_channel2: MidiChannel::from_value(values[1]),
            midi_channel3: MidiChannel::from_value(values[2]),
            midi_channel4: MidiChannel::from_value(values[3]),
            midi_out: MidiOut::from_value(values[4]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel1.into()).unwrap();
        vec.push(self.midi_channel2.into()).unwrap();
        vec.push(self.midi_channel3.into()).unwrap();
        vec.push(self.midi_channel4.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Backward,
    PingPong,
    Random,
}

impl Direction {
    fn from_fader(value: u16) -> Self {
        match value / 1024 {
            0 => Direction::Forward,
            1 => Direction::Backward,
            2 => Direction::PingPong,
            _ => Direction::Random,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    seq: Arr<u16, 64>,
    gateseq: Arr<bool, 64>,
    legato_seq: Arr<bool, 64>,
    // Alt layer - fader-scale values (0-4095)
    length_fader: [u16; 4],      // F0: derive seq_length = val/256+1
    gate_fader: [u16; 4],        // F1: derive gate_length
    oct_fader: [u16; 4],         // F2: derive oct = val/1000
    range_fader: [u16; 4],       // F3: derive range = val/1000+1
    res_fader: [u16; 4],         // F4: derive res_index = val/512
    direction_fader: [u16; 4],   // F5: derive Direction = val/1024
    probability_fader: [u16; 4], // F6: probability 5%..100% (linear)
    slide_fader: [u16; 4],       // F7: 303-style slide time (0 = instant)
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            seq: Arr::new([0; 64]),
            gateseq: Arr::new([true; 64]),
            legato_seq: Arr::new([false; 64]),
            // Default fader values - positioned to produce sensible defaults
            length_fader: [3840; 4],      // -> length 16 (3840/256+1 = 16)
            gate_fader: [2032; 4],        // -> gate_length 127 (127*16 = 2032)
            oct_fader: [0; 4],            // -> oct 0
            range_fader: [2000; 4],       // -> range 3 (2000/1000+1 = 3)
            res_fader: [2048; 4],         // -> res_index 4 (2048/512 = 4)
            direction_fader: [0; 4],      // -> Direction::Forward
            probability_fader: [4095; 4], // -> 100%
            slide_fader: [0; 4],          // -> instant (no slide)
        }
    }
}

/// Maps the per-track probability fader (0..=4095) to a die-roll threshold (0..=4096).
/// Fader 0 → 5%, fader 4095 → 100%.
fn probability_threshold(fader: u16) -> u16 {
    205 + ((fader as u32 * 3891) / 4095) as u16
}

/// 303-style slide coefficient. RC-filter exponential approach. Same approach as
/// midi2cv's glide: coeff = 1 - exp(-1/tau), applied each 1ms tick.
/// Fader 0 → 1.0 (instant). Fader 4095 → tau ~205 ticks (~600ms to settle).
fn calc_slide_coeff(fader: u16) -> f32 {
    if fader == 0 {
        1.0
    } else {
        let tau = 1.0 + (fader as f32 * 0.05);
        1.0 - expf(-1.0 / tau)
    }
}

/// Maps a raw step counter to a position within `length`, respecting `direction`.
fn step_position(direction: Direction, step: usize, length: u8, die_roll: u16) -> usize {
    let l = length as usize;
    if l == 0 {
        return 0;
    }
    match direction {
        Direction::Forward => step % l,
        Direction::Backward => (l - 1) - (step % l),
        Direction::PingPong => {
            // Endpoints are repeated. For L=3: 0,1,2,2,1,0,0,1,2,2,1,0,...
            // Period = 2*L; first half ascends, second half descends.
            let period = 2 * l;
            let phase = step % period;
            if phase < l {
                phase
            } else {
                period - 1 - phase
            }
        }
        Direction::Random => (die_roll as usize) % l,
    }
}

impl AppStorage for Storage {}

/// Derives runtime parameters from stored fader values.
fn derive_runtime_params(
    length_faders: [u16; 4],
    gate_faders: [u16; 4],
    res_faders: [u16; 4],
    resolution: &[usize; 8],
) -> ([u8; 4], [usize; 4], [u8; 4]) {
    let mut seq_length = [0u8; 4];
    let mut clockres = [0usize; 4];
    let mut gatel = [0u8; 4];
    for n in 0..4 {
        seq_length[n] = (length_faders[n] / 256 + 1) as u8;
        clockres[n] = resolution[(res_faders[n] / 512) as usize];
        gatel[n] = (clockres[n] * (gate_faders[n] as usize) / 4096) as u8;
        gatel[n] = gatel[n].clamp(1, clockres[n] as u8 - 1);
    }
    (seq_length, clockres, gatel)
}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            midi_channel1: MidiChannel::from(1),
            midi_channel2: MidiChannel::from(2),
            midi_channel3: MidiChannel::from(3),
            midi_channel4: MidiChannel::from(4),
            midi_out: MidiOut::default(),
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
    let range = Range::_0_10V;
    let (midi_out, midi_chan1, midi_chan2, midi_chan3, midi_chan4) = params.query(|p| {
        (
            p.midi_out,
            p.midi_channel1,
            p.midi_channel2,
            p.midi_channel3,
            p.midi_channel4,
        )
    });

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let mut clk = app.use_clock();
    let die = app.use_die();
    let ticks = clk.get_ticker();
    let led = app.use_leds();

    let midi = [
        app.use_midi_output(midi_out, midi_chan1, false),
        app.use_midi_output(midi_out, midi_chan2, false),
        app.use_midi_output(midi_out, midi_chan3, false),
        app.use_midi_output(midi_out, midi_chan4, false),
    ];

    let cv_out = [
        app.make_out_jack(0, Range::_0_10V).await,
        app.make_out_jack(2, Range::_0_10V).await,
        app.make_out_jack(4, Range::_0_10V).await,
        app.make_out_jack(6, Range::_0_10V).await,
    ];
    let gate_out = [
        app.make_gate_jack(1, 4095).await,
        app.make_gate_jack(3, 4095).await,
        app.make_gate_jack(5, 4095).await,
        app.make_gate_jack(7, 4095).await,
    ];

    let quantizer = app.use_quantizer(range);

    let page_glob: Global<usize> = app.make_global(0);
    let latch_layer_glob: Global<LatchLayer> = app.make_global(LatchLayer::Main);
    let seq_glob: Global<[u16; 64]> = app.make_global([0; 64]);
    let gateseq_glob: Global<[bool; 64]> = app.make_global([true; 64]);
    let legatoseq_glob: Global<[bool; 64]> = app.make_global([false; 64]);

    let seq_length_glob: Global<[u8; 4]> = app.make_global([16; 4]);
    let gatelength_glob: Global<[u8; 4]> = app.make_global([128; 4]);
    let clockres_glob = app.make_global([6usize; 4]);
    let direction_glob: Global<[Direction; 4]> = app.make_global([Direction::Forward; 4]);
    // Cached die-roll thresholds (0..=4096); recomputed when F6 changes.
    let probability_glob: Global<[u16; 4]> = app.make_global([4096; 4]);
    // Per-tick exponential coefficient for the slide (1.0 = instant).
    let slide_coeff_glob: Global<[f32; 4]> = app.make_global([1.0; 4]);
    // CV target set by clock_handler at note-on; slide_handler interpolates toward it.
    let target_cv_glob: Global<[u16; 4]> = app.make_global([0; 4]);
    // Set true at note-on when the previously played step had legato enabled.
    let sliding_glob: Global<[bool; 4]> = app.make_global([false; 4]);
    // Actual playing position per track (post direction). LED display reads this
    // so the highlighted step matches the audible step regardless of direction.
    let playing_pos_glob: Global<[usize; 4]> = app.make_global([0; 4]);

    let resolution = [24usize, 16, 12, 8, 6, 4, 3, 2];

    let mut lastnote = [MidiNote::default(); 4];
    let mut last_step_index = [0usize; 4];
    // Was the just-played step a legato step? Drives whether to slide into the next.
    let mut prev_step_legato = [false; 4];
    let mut gatelength1 = gatelength_glob.get();

    // Initialize latches for all 8 faders
    let mut latches: [libfp::latch::AnalogLatch; 8] =
        core::array::from_fn(|i| app.make_latch(faders.get_value_at(i)));

    let (
        seq_saved,
        gateseq_saved,
        legato_seq_saved,
        length_faders,
        gate_faders,
        _oct_faders,
        _range_faders,
        res_faders,
        direction_faders,
        probability_faders,
        slide_faders,
    ) = storage.query(|s| {
        (
            s.seq,
            s.gateseq,
            s.legato_seq,
            s.length_fader,
            s.gate_fader,
            s.oct_fader,
            s.range_fader,
            s.res_fader,
            s.direction_fader,
            s.probability_fader,
            s.slide_fader,
        )
    });

    seq_glob.set(seq_saved.get());
    gateseq_glob.set(gateseq_saved.get());
    legatoseq_glob.set(legato_seq_saved.get());

    let (seq_length_init, clockres_init, gatel_init) =
        derive_runtime_params(length_faders, gate_faders, res_faders, &resolution);
    seq_length_glob.set(seq_length_init);
    clockres_glob.set(clockres_init);
    gatelength_glob.set(gatel_init);
    direction_glob.set(direction_faders.map(Direction::from_fader));
    probability_glob.set(probability_faders.map(probability_threshold));
    slide_coeff_glob.set(slide_faders.map(calc_slide_coeff));

    let shift_handler = async {
        loop {
            app.delay_millis(16).await;
            let layer = if buttons.is_shift_pressed() {
                LatchLayer::Alt
            } else {
                LatchLayer::Main
            };
            latch_layer_glob.set(layer);
        }
    };

    let fader_handler = async {
        loop {
            let chan = faders.wait_for_any_change().await;
            let page = page_glob.get();
            let seq_idx = page / 2;
            let latch_layer = latch_layer_glob.get();

            let target_value = match latch_layer {
                LatchLayer::Main => {
                    let seq = seq_glob.get();
                    seq[chan + (page * 8)]
                }
                LatchLayer::Alt => get_alt_target(chan, seq_idx, storage),
                LatchLayer::Third => 0,
            };

            if let Some(new_value) =
                latches[chan].update(faders.get_value_at(chan), latch_layer, target_value)
            {
                match latch_layer {
                    LatchLayer::Main => {
                        let mut seq = seq_glob.get();
                        seq[chan + (page * 8)] = new_value;
                        seq_glob.set(seq);
                        storage.modify_and_save(|s| {
                            let mut seq_arr = s.seq.get();
                            seq_arr[chan + (page * 8)] = new_value;
                            s.seq.set(seq_arr);
                        });
                    }
                    LatchLayer::Alt => {
                        apply_alt_update(
                            chan,
                            seq_idx,
                            new_value,
                            &AltUpdateContext {
                                storage,
                                seq_length_glob: &seq_length_glob,
                                gatelength_glob: &gatelength_glob,
                                clockres_glob: &clockres_glob,
                                direction_glob: &direction_glob,
                                probability_glob: &probability_glob,
                                slide_coeff_glob: &slide_coeff_glob,
                                resolution: &resolution,
                            },
                        );
                    }
                    LatchLayer::Third => {}
                }
            }
        }
    };

    let button_handler = async {
        loop {
            let (chan, is_shift_pressed) = buttons.wait_for_any_down().await;
            let page = page_glob.get();

            if !is_shift_pressed {
                let mut gateseq = gateseq_glob.get();
                let mut legato_seq = legatoseq_glob.get();

                gateseq[chan + (page * 8)] = !gateseq[chan + (page * 8)];
                legato_seq[chan + (page * 8)] = false;

                gateseq_glob.set(gateseq);
                legatoseq_glob.set(legato_seq);

                storage.modify_and_save(|s| {
                    s.gateseq.set(gateseq);
                    s.legato_seq.set(legato_seq);
                });
            } else {
                page_glob.set(chan);
            }
        }
    };

    let button_long_press_handler = async {
        loop {
            let (chan, is_shift_pressed) = buttons.wait_for_any_long_press().await;
            let page = page_glob.get();

            if !is_shift_pressed {
                let mut legato_seq = legatoseq_glob.get();
                let mut gateseq = gateseq_glob.get();

                legato_seq[chan + (page * 8)] = !legato_seq[chan + (page * 8)];
                gateseq[chan + (page * 8)] = true;

                legatoseq_glob.set(legato_seq);
                gateseq_glob.set(gateseq);

                storage.modify_and_save(|s| {
                    s.gateseq.set(gateseq);
                    s.legato_seq.set(legato_seq);
                });
            }
        }
    };

    let led_handler = async {
        let intensities = [
            Brightness::Low,
            Brightness::Mid,
            Brightness::High,
            Brightness::High,
        ];
        let colors = [Color::Yellow, Color::Pink, Color::Cyan, Color::White];

        loop {
            app.delay_millis(16).await;
            let clockres = clockres_glob.get();
            let clockn = ticks() as usize;
            let page = page_glob.get();

            if buttons.is_shift_pressed() {
                let seq_length = seq_length_glob.get();
                let playing_pos = playing_pos_glob.get();
                let current_step = playing_pos[page / 2] as u8;

                for n in 0..8 {
                    let bright = if n == page {
                        intensities[3]
                    } else {
                        intensities[1]
                    };
                    led.set(n, Led::Button, colors[n / 2], bright);
                }
                let led_color = if matches!(clockres[page / 2], 2 | 4 | 8 | 16) {
                    Color::Orange
                } else {
                    Color::Blue
                };
                for n in 0..=15 {
                    let mut bright = Brightness::Off;
                    if n < seq_length[page / 2] {
                        bright = Brightness::Mid;
                    }
                    if n == current_step {
                        bright = Brightness::High;
                    }
                    if n >= seq_length[page / 2] {
                        bright = Brightness::Off;
                    }
                    if n < 8 {
                        led.set(n as usize, Led::Top, led_color, bright)
                    } else {
                        led.set(n as usize - 8, Led::Bottom, led_color, bright)
                    }
                }
            } else {
                let seq = seq_glob.get();
                let gateseq = gateseq_glob.get();
                let legato_seq = legatoseq_glob.get();
                let seq_length = seq_length_glob.get();
                let playing_pos = playing_pos_glob.get();
                let color = colors[page / 2];

                for n in 0..8 {
                    led.set(
                        n,
                        Led::Top,
                        color,
                        Brightness::Custom((seq[n + (page * 8)] / 16) as u8 / 2),
                    );

                    let button_bright = if legato_seq[n + (page * 8)] {
                        intensities[2]
                    } else if gateseq[n + (page * 8)] {
                        intensities[1]
                    } else {
                        intensities[0]
                    };
                    led.set(n, Led::Button, color, button_bright);

                    let index = seq_length[page / 2] as usize - (page % 2 * 8);
                    if n >= index || index > 16 {
                        led.unset(n, Led::Button);
                    }

                    // Show which page of each track is currently playing
                    let track = n / 2;
                    let step = playing_pos[track];
                    let active = if n % 2 == 0 { step < 8 } else { step >= 8 };
                    if active {
                        led.set(n, Led::Bottom, Color::Red, Brightness::Mid);
                    } else {
                        led.unset(n, Led::Bottom);
                    }
                }

                // Highlight the current step button on the active page
                let step_in_seq = playing_pos[page / 2];
                let page_offset = (page % 2) * 8;
                if clockn != 0 && step_in_seq >= page_offset && step_in_seq < page_offset + 8 {
                    led.set(
                        step_in_seq - page_offset,
                        Led::Button,
                        Color::Red,
                        Brightness::Mid,
                    );
                }

                led.set(page, Led::Bottom, color, Brightness::High);
            }
        }
    };

    let clock_handler = async {
        loop {
            let gateseq = gateseq_glob.get();
            let seq_length = seq_length_glob.get();
            let clockres = clockres_glob.get();
            let legato_seq = legatoseq_glob.get();

            match clk.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset | ClockEvent::Stop => {
                    for n in 0..4 {
                        midi[n].send_note_off(lastnote[n]).await;
                        gate_out[n].set_low().await;
                    }
                }
                ClockEvent::Tick => {
                    let clockn = ticks() as usize;
                    let seq = seq_glob.get();
                    let direction = direction_glob.get();
                    let probability = probability_glob.get();
                    for n in 0..4 {
                        if clockn.is_multiple_of(clockres[n]) {
                            let step = clockn / clockres[n];
                            let pos = step_position(direction[n], step, seq_length[n], die.roll());
                            let clkindex = pos + (n * 16);
                            last_step_index[n] = clkindex;
                            let mut positions = playing_pos_glob.get();
                            positions[n] = pos;
                            playing_pos_glob.set(positions);

                            midi[n].send_note_off(lastnote[n]).await;
                            let fires = gateseq[clkindex] && die.roll() < probability[n];
                            if fires {
                                let out = quantizer
                                    .get_quantized_note(
                                        (seq[clkindex] as u32
                                            * ((storage.query(|s| s.range_fader[n]) / 1000 + 1)
                                                as u32)
                                            * 410
                                            / 4095) as u16
                                            + (storage.query(|s| s.oct_fader[n]) / 1000) * 410,
                                    )
                                    .await;
                                lastnote[n] = out.as_midi();

                                midi[n].send_note_on(lastnote[n], 4095).await;
                                gatelength1 = gatelength_glob.get();
                                // Hand the target CV to slide_handler; slide only if
                                // the previously played step had legato enabled.
                                let mut targets = target_cv_glob.get();
                                targets[n] = out.as_counts(range);
                                target_cv_glob.set(targets);
                                let mut slid = sliding_glob.get();
                                slid[n] = prev_step_legato[n];
                                sliding_glob.set(slid);
                                gate_out[n].set_high().await;
                            } else {
                                gate_out[n].set_low().await;
                            }
                            prev_step_legato[n] = legato_seq[clkindex];
                        }
                        if clockn >= gatelength1[n] as usize
                            && (clockn - gatelength1[n] as usize).is_multiple_of(clockres[n])
                        {
                            let clkindex = last_step_index[n];
                            if gateseq[clkindex] && !legato_seq[clkindex] {
                                gate_out[n].set_low().await;
                                midi[n].send_note_off(lastnote[n]).await;
                            }
                        }
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

                    let (
                        seq_saved,
                        gateseq_saved,
                        legato_seq_saved,
                        length_faders,
                        gate_faders,
                        res_faders,
                        direction_faders,
                        probability_faders,
                        slide_faders,
                    ) = storage.query(|s| {
                        (
                            s.seq,
                            s.gateseq,
                            s.legato_seq,
                            s.length_fader,
                            s.gate_fader,
                            s.res_fader,
                            s.direction_fader,
                            s.probability_fader,
                            s.slide_fader,
                        )
                    });

                    seq_glob.set(seq_saved.get());
                    gateseq_glob.set(gateseq_saved.get());
                    legatoseq_glob.set(legato_seq_saved.get());

                    let (seq_length, clockres, gatel) =
                        derive_runtime_params(length_faders, gate_faders, res_faders, &resolution);
                    seq_length_glob.set(seq_length);
                    clockres_glob.set(clockres);
                    gatelength_glob.set(gatel);
                    direction_glob.set(direction_faders.map(Direction::from_fader));
                    probability_glob.set(probability_faders.map(probability_threshold));
                    slide_coeff_glob.set(slide_faders.map(calc_slide_coeff));
                }
                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    // CV slide handler — runs at 1ms and exponentially interpolates each
    // track's CV from its current value toward target_cv_glob[n] using
    // slide_coeff_glob[n]. Only slides when sliding_glob[n] is true (set
    // by clock_handler when the previously played step had legato).
    let slide_handler = async {
        let mut current: [f32; 4] = [0.0; 4];
        loop {
            app.delay_millis(1).await;
            let target = target_cv_glob.get();
            let sliding = sliding_glob.get();
            let coeff = slide_coeff_glob.get();
            for n in 0..4 {
                let target_f = target[n] as f32;
                if sliding[n] && coeff[n] < 1.0 {
                    current[n] += (target_f - current[n]) * coeff[n];
                } else {
                    current[n] = target_f;
                }
                cv_out[n].set_value(current[n] as u16);
            }
        }
    };

    join4(
        join5(
            shift_handler,
            fader_handler,
            button_handler,
            led_handler,
            clock_handler,
        ),
        button_long_press_handler,
        scene_handler,
        slide_handler,
    )
    .await;
}

fn get_alt_target(chan: usize, seq_idx: usize, storage: &ManagedStorage<Storage>) -> u16 {
    match chan {
        0 => storage.query(|s| s.length_fader[seq_idx]),
        1 => storage.query(|s| s.gate_fader[seq_idx]),
        2 => storage.query(|s| s.oct_fader[seq_idx]),
        3 => storage.query(|s| s.range_fader[seq_idx]),
        4 => storage.query(|s| s.res_fader[seq_idx]),
        5 => storage.query(|s| s.direction_fader[seq_idx]),
        6 => storage.query(|s| s.probability_fader[seq_idx]),
        7 => storage.query(|s| s.slide_fader[seq_idx]),
        _ => 0,
    }
}

struct AltUpdateContext<'a> {
    storage: &'a ManagedStorage<Storage>,
    seq_length_glob: &'a Global<[u8; 4]>,
    gatelength_glob: &'a Global<[u8; 4]>,
    clockres_glob: &'a Global<[usize; 4]>,
    direction_glob: &'a Global<[Direction; 4]>,
    probability_glob: &'a Global<[u16; 4]>,
    slide_coeff_glob: &'a Global<[f32; 4]>,
    resolution: &'a [usize; 8],
}

fn apply_alt_update(chan: usize, seq_idx: usize, value: u16, ctx: &AltUpdateContext) {
    match chan {
        0 => {
            // Sequence length
            ctx.storage
                .modify_and_save(|s| s.length_fader[seq_idx] = value);
            let mut arr = ctx.seq_length_glob.get();
            arr[seq_idx] = (value / 256 + 1) as u8;
            ctx.seq_length_glob.set(arr);
        }
        1 => {
            // Gate length
            ctx.storage
                .modify_and_save(|s| s.gate_fader[seq_idx] = value);
            let clockres = ctx.clockres_glob.get();
            let mut arr = ctx.gatelength_glob.get();
            arr[seq_idx] = (clockres[seq_idx] * (value as usize) / 4096) as u8;
            arr[seq_idx] = arr[seq_idx].clamp(1, clockres[seq_idx] as u8 - 1);
            ctx.gatelength_glob.set(arr);
        }
        2 => {
            // Octave
            ctx.storage
                .modify_and_save(|s| s.oct_fader[seq_idx] = value);
        }
        3 => {
            // Range
            ctx.storage
                .modify_and_save(|s| s.range_fader[seq_idx] = value);
        }
        4 => {
            // Resolution
            ctx.storage
                .modify_and_save(|s| s.res_fader[seq_idx] = value);
            let res_index = (value / 512) as usize;
            let mut arr = ctx.clockres_glob.get();
            arr[seq_idx] = ctx.resolution[res_index];
            ctx.clockres_glob.set(arr);
            // Re-clamp gate length within new resolution
            let clockres = ctx.clockres_glob.get();
            let mut gatel = ctx.gatelength_glob.get();
            gatel[seq_idx] = gatel[seq_idx].clamp(1, clockres[seq_idx] as u8);
            ctx.gatelength_glob.set(gatel);
        }
        5 => {
            // Direction
            ctx.storage
                .modify_and_save(|s| s.direction_fader[seq_idx] = value);
            let mut arr = ctx.direction_glob.get();
            arr[seq_idx] = Direction::from_fader(value);
            ctx.direction_glob.set(arr);
        }
        6 => {
            // Probability
            ctx.storage
                .modify_and_save(|s| s.probability_fader[seq_idx] = value);
            let mut arr = ctx.probability_glob.get();
            arr[seq_idx] = probability_threshold(value);
            ctx.probability_glob.set(arr);
        }
        7 => {
            // Slide time
            ctx.storage
                .modify_and_save(|s| s.slide_fader[seq_idx] = value);
            let mut arr = ctx.slide_coeff_glob.get();
            arr[seq_idx] = calc_slide_coeff(value);
            ctx.slide_coeff_glob.set(arr);
        }
        _ => {}
    }
}
