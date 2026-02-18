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

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_channel1: MidiChannel::from(1),
            midi_channel2: MidiChannel::from(2),
            midi_channel3: MidiChannel::from(3),
            midi_channel4: MidiChannel::from(4),
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
    let led = app.use_leds();

    let midi = [
        app.use_midi_output(midi_out, midi_chan1),
        app.use_midi_output(midi_out, midi_chan2),
        app.use_midi_output(midi_out, midi_chan3),
        app.use_midi_output(midi_out, midi_chan4),
    ];

    let clockn_glob = app.make_global(0);

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
    let led_flag_glob: Global<bool> = app.make_global(true);
    let latch_layer_glob: Global<LatchLayer> = app.make_global(LatchLayer::Main);
    let seq_glob: Global<[u16; 64]> = app.make_global([0; 64]);
    let gateseq_glob: Global<[bool; 64]> = app.make_global([true; 64]);
    let legatoseq_glob: Global<[bool; 64]> = app.make_global([false; 64]);

    let seq_length_glob: Global<[u8; 4]> = app.make_global([16; 4]);
    let gatelength_glob: Global<[u8; 4]> = app.make_global([128; 4]);

    let clockres_glob = app.make_global([6, 6, 6, 6]);

    let resolution = [24, 16, 12, 8, 6, 4, 3, 2];

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

    // Derive runtime parameters from fader values
    let mut seq_length_saved = [0u8; 4];
    let mut clockres = [0usize; 4];
    let mut gatel = [0u8; 4];
    for n in 0..4 {
        seq_length_saved[n] = (length_faders[n] / 256 + 1) as u8;
        clockres[n] = resolution[(res_faders[n] / 512) as usize];
        gatel[n] = (clockres[n] * (gate_faders[n] as usize) / 4096) as u8;
        gatel[n] = gatel[n].clamp(1, clockres[n] as u8 - 1);
    }
    seq_length_glob.set(seq_length_saved);
    clockres_glob.set(clockres);
    gatelength_glob.set(gatel);

    let shift_handler = async {
        loop {
            app.delay_millis(1).await;
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

            // Determine target value based on layer and fader
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
                        // Update step value
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
            led_flag_glob.set(true);
        }
    };

    let button_handler = async {
        loop {
            let (chan, is_shift_pressed) = buttons.wait_for_any_down().await;
            let mut gateseq = gateseq_glob.get();
            let mut legato_seq = legatoseq_glob.get();

            // let mut gateseq = gateseq_glob.get_array();
            let page = page_glob.get();
            if !is_shift_pressed {
                gateseq[chan + (page * 8)] = !gateseq[chan + (page * 8)];
                gateseq_glob.set(gateseq);

                legato_seq[chan + (page * 8)] = false;
                legatoseq_glob.set(legato_seq);

                storage.modify_and_save(|s| {
                    s.gateseq.set(gateseq);
                    s.legato_seq.set(legato_seq);
                });

                // gateseq_glob.set_array(gateseq);
                // gateseq_glob.save();
                led_flag_glob.set(true);
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
                legato_seq[chan + (page * 8)] = !legato_seq[chan + (page * 8)];
                legatoseq_glob.set(legato_seq);

                let mut gateseq = gateseq_glob.get();
                gateseq[chan + (page * 8)] = true;
                gateseq_glob.set(gateseq);

                storage.modify_and_save(|s| s.gateseq.set(gateseq));
                storage.modify_and_save(|s| s.legato_seq.set(legato_seq));

                // gateseq_glob.set_array(gateseq);
                // gateseq_glob.save();
            }
        }
    };

    let led_handler = async {
        loop {
            let intensities = [
                Brightness::Low,
                Brightness::Mid,
                Brightness::High,
                Brightness::High,
            ];
            let colors = [Color::Yellow, Color::Pink, Color::Cyan, Color::White];
            app.delay_millis(16).await;
            let clockres = clockres_glob.get();

            if buttons.is_shift_pressed() {
                let clockn = clockn_glob.get();

                let seq_length = seq_length_glob.get();

                let page = page_glob.get();
                let mut bright = Brightness::Mid;
                for n in 0..=7 {
                    if n == page {
                        bright = intensities[3];
                    } else {
                        bright = intensities[1];
                    }
                    led.set(n, Led::Button, colors[n / 2], bright);
                }
                for n in 0..=15 {
                    if n < seq_length[page / 2] {
                        bright = Brightness::Mid;
                    }
                    if n == (clockn / clockres[page / 2]) as u8 % seq_length[page / 2] {
                        bright = Brightness::High;
                    }
                    if n >= seq_length[page / 2] {
                        bright = Brightness::Off;
                    }

                    let led_color = if matches!(clockres[page / 2], 2 | 4 | 8 | 16) {
                        Color::Orange
                    } else {
                        Color::Blue
                    };
                    if n < 8 {
                        led.set(n as usize, Led::Top, led_color, bright)
                    } else {
                        led.set(n as usize - 8, Led::Bottom, led_color, bright)
                    }
                }
            }

            if !buttons.is_shift_pressed() {
                // LED stuff
                let page = page_glob.get();

                let seq = seq_glob.get();
                let gateseq = gateseq_glob.get();
                let seq_length = seq_length_glob.get();

                let mut color = colors[0];
                let clockn = clockn_glob.get(); // this should go

                if page / 2 == 0 {
                    color = colors[0];
                }
                if page / 2 == 1 {
                    color = colors[1];
                }
                if page / 2 == 2 {
                    color = colors[2];
                }
                if page / 2 == 3 {
                    color = colors[3];
                }

                let legato_seq = legatoseq_glob.get();

                for n in 0..=7 {
                    led.set(
                        n,
                        Led::Top,
                        color,
                        Brightness::Custom((seq[n + (page * 8)] / 16) as u8 / 2),
                    );

                    if gateseq[n + (page * 8)] {
                        led.set(n, Led::Button, color, intensities[1]);
                    }
                    if !gateseq[n + (page * 8)] {
                        led.set(n, Led::Button, color, intensities[0]);
                    }
                    if legato_seq[n + (page * 8)] {
                        led.set(n, Led::Button, color, intensities[2]);
                    }

                    let index = seq_length[page / 2] as usize - (page % 2 * 8);

                    if n >= index || index > 16 {
                        led.unset(n, Led::Button);
                    }

                    if (clockn / clockres[n / 2] % seq_length[n / 2] as usize) % 16 - (n % 2) * 8
                        < 8
                    {
                        //this needs changing
                        led.set(n, Led::Bottom, Color::Red, Brightness::Mid)
                    } else {
                        led.unset(n, Led::Bottom);
                    }
                }
                //runing light on buttons
                if ((clockn / clockres[page / 2]) % seq_length[page / 2] as usize) % 16
                    - (page % 2) * 8
                    < 8
                    && clockn != 0
                {
                    led.set(
                        (clockn / clockres[page / 2] % seq_length[page / 2] as usize) % 16
                            - (page % 2) * 8,
                        Led::Button,
                        Color::Red,
                        Brightness::Mid,
                    );
                }

                led.set(page, Led::Bottom, color, Brightness::High);
            }

            led_flag_glob.set(false);
        }
    };

    let clock_handler = async {
        loop {
            let gateseq = gateseq_glob.get();
            let seq_length = seq_length_glob.get();
            let clockres = clockres_glob.get();
            let legato_seq = legatoseq_glob.get();

            let mut clockn = clockn_glob.get();

            match clk.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset => {
                    clockn = 0;
                    for n in 0..4 {
                        midi[n].send_note_off(lastnote[n]).await;
                        gate_out[n].set_low().await;
                    }
                }
                ClockEvent::Tick => {
                    for n in 0..=3 {
                        if clockn.is_multiple_of(clockres[n]) {
                            let clkindex =
                                (clockn / clockres[n] % seq_length[n] as usize) + (n * 16);
                            midi[n].send_note_off(lastnote[n]).await;
                            if gateseq[clkindex] {
                                let seq = seq_glob.get();

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
                        if (clockn - gatelength1[n] as usize).is_multiple_of(clockres[n]) {
                            let clkindex =
                                (((clockn - 1) / clockres[n]) % seq_length[n] as usize) + (n * 16);
                            if gateseq[clkindex] && !legato_seq[clkindex] {
                                gate_out[n].set_low().await;
                                midi[n].send_note_off(lastnote[n]).await;
                            }
                        }
                    }
                    clockn += 1;
                }
                _ => {}
            }

            clockn_glob.set(clockn);
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

                    // Derive runtime parameters from fader values
                    let mut seq_length_saved = [0u8; 4];
                    let mut clockres = [0usize; 4];
                    let mut gatel = [0u8; 4];
                    for n in 0..4 {
                        seq_length_saved[n] = (length_faders[n] / 256 + 1) as u8;
                        clockres[n] = resolution[(res_faders[n] / 512) as usize];
                        gatel[n] = (clockres[n] * (gate_faders[n] as usize) / 4096) as u8;
                        gatel[n] = gatel[n].clamp(1, clockres[n] as u8 - 1);
                    }
                    seq_length_glob.set(seq_length_saved);
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
            // Update gate length to clamp within new resolution
            let clockres = ctx.clockres_glob.get();
            let mut gatel = ctx.gatelength_glob.get();
            gatel[seq_idx] = gatel[seq_idx].clamp(1, clockres[seq_idx] as u8);
            ctx.gatelength_glob.set(gatel);
        }
        _ => {}
    }
}
