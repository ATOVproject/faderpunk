use embassy_futures::{
    join::{join3, join5},
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
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

#[derive(Serialize, Deserialize)]
pub struct Storage {
    seq: Arr<u16, 64>,
    gateseq: Arr<bool, 64>,
    legato_seq: Arr<bool, 64>,
    // Alt layer - fader-scale values (0-4095)
    length_fader: [u16; 4], // F0: derive seq_length = val/256+1
    gate_fader: [u16; 4],   // F1: derive gate_length
    oct_fader: [u16; 4],    // F2: derive oct = val/1000
    range_fader: [u16; 4],  // F3: derive range = val/1000+1
    res_fader: [u16; 4],    // F4: derive res_index = val/512
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            seq: Arr::new([0; 64]),
            gateseq: Arr::new([true; 64]),
            legato_seq: Arr::new([false; 64]),
            // Default fader values - positioned to produce sensible defaults
            length_fader: [3840; 4], // -> length 16 (3840/256+1 = 16)
            gate_fader: [2032; 4],   // -> gate_length 127 (127*16 = 2032)
            oct_fader: [0; 4],       // -> oct 0
            range_fader: [2000; 4],  // -> range 3 (2000/1000+1 = 3)
            res_fader: [2048; 4],    // -> res_index 4 (2048/512 = 4)
        }
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

    let resolution = [24usize, 16, 12, 8, 6, 4, 3, 2];

    let mut lastnote = [MidiNote::default(); 4];
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
                let track = page / 2;
                let current_step = (clockn / clockres[track] % seq_length[track] as usize) as u8;

                for n in 0..8 {
                    let bright = if n == page {
                        intensities[3]
                    } else {
                        intensities[1]
                    };
                    led.set(n, Led::Button, colors[n / 2], bright);
                }
                for n in 0..=15 {
                    let mut bright = Brightness::Off;
                    if n < seq_length[page / 2] {
                        bright = Brightness::Mid;
                    }
                    if n == (clockn / clockres[page / 2]) as u8 % seq_length[page / 2] {
                        bright = Brightness::High;
                    }
                    if n >= seq_length[page / 2] {
                        bright = Brightness::Off;
                    }
                    if n < 8 {
                        led.set(n as usize, Led::Top, Color::Red, bright)
                    } else {
                        led.set(n as usize - 8, Led::Bottom, Color::Red, bright)
                    }
                }
            } else {
                let seq = seq_glob.get();
                let gateseq = gateseq_glob.get();
                let legato_seq = legatoseq_glob.get();
                let seq_length = seq_length_glob.get();
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
                    let step = clockn / clockres[track] % seq_length[track] as usize;
                    let active = if n % 2 == 0 { step < 8 } else { step >= 8 };
                    if active {
                        led.set(n, Led::Bottom, Color::Red, Brightness::Mid);
                    } else {
                        led.unset(n, Led::Bottom);
                    }
                }

                // Highlight the current step button on the active page
                let step_in_seq =
                    (clockn / clockres[page / 2] % seq_length[page / 2] as usize) % 16;
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
                    for n in 0..4 {
                        if clockn.is_multiple_of(clockres[n]) {
                            let clkindex =
                                (clockn / clockres[n] % seq_length[n] as usize) + (n * 16);

                            midi[n].send_note_off(lastnote[n]).await;
                            if gateseq[clkindex] {
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
                                cv_out[n].set_value(out.as_counts(range));
                                gate_out[n].set_high().await;
                            } else {
                                gate_out[n].set_low().await;
                            }
                        }
                        if clockn >= gatelength1[n] as usize
                            && (clockn - gatelength1[n] as usize).is_multiple_of(clockres[n])
                        {
                            let clkindex =
                                (((clockn - 1) / clockres[n]) % seq_length[n] as usize) + (n * 16);
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
                    ) = storage.query(|s| {
                        (
                            s.seq,
                            s.gateseq,
                            s.legato_seq,
                            s.length_fader,
                            s.gate_fader,
                            s.res_fader,
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
                }
                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    join3(
        join5(
            shift_handler,
            fader_handler,
            button_handler,
            led_handler,
            clock_handler,
        ),
        button_long_press_handler,
        scene_handler,
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
        _ => 0, // F5-F7 have no alt function
    }
}

struct AltUpdateContext<'a> {
    storage: &'a ManagedStorage<Storage>,
    seq_length_glob: &'a Global<[u8; 4]>,
    gatelength_glob: &'a Global<[u8; 4]>,
    clockres_glob: &'a Global<[usize; 4]>,
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
        _ => {}
    }
}
