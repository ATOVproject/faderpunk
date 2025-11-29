use embassy_futures::{
    join::{join, join5},
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    latch::{AnalogLatch, LatchLayer},
    AppIcon, Brightness, ClockDivision, Color, Config, Param, Range, Value, APP_MAX_PARAMS,
};

use crate::app::{
    App, AppParams, AppStorage, Arr, ClockEvent, Global, Led, ManagedStorage, ParamStore,
    SceneEvent,
};

pub const CHANNELS: usize = 8;
pub const PARAMS: usize = 4;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Sequencer",
    "4 x 16 step CV/gate sequencers",
    Color::Yellow,
    AppIcon::Sequence,
)
.add_param(Param::i32 {
    name: "MIDI Channel 1",
    min: 1,
    max: 16,
})
.add_param(Param::i32 {
    name: "MIDI Channel 2",
    min: 1,
    max: 16,
})
.add_param(Param::i32 {
    name: "MIDI Channel 3",
    min: 1,
    max: 16,
})
.add_param(Param::i32 {
    name: "MIDI Channel 4",
    min: 1,
    max: 16,
});

pub struct Params {
    midi_channel1: i32,
    midi_channel2: i32,
    midi_channel3: i32,
    midi_channel4: i32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_channel1: 1,
            midi_channel2: 2,
            midi_channel3: 3,
            midi_channel4: 4,
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            midi_channel1: i32::from_value(values[0]),
            midi_channel2: i32::from_value(values[1]),
            midi_channel3: i32::from_value(values[2]),
            midi_channel4: i32::from_value(values[3]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel1.into()).unwrap();
        vec.push(self.midi_channel2.into()).unwrap();
        vec.push(self.midi_channel3.into()).unwrap();
        vec.push(self.midi_channel4.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    seq: Arr<u16, 64>,
    gateseq: Arr<bool, 64>,
    legato_seq: Arr<bool, 64>,
    seq_length: [u8; 4],
    seqres: [usize; 4],
    gate_length: [u8; 4],
    range: [u8; 4],
    oct: [u8; 4],
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            seq: Arr::new([0; 64]),
            gateseq: Arr::new([true; 64]),
            legato_seq: Arr::new([false; 64]),
            seq_length: [16; 4],
            seqres: [4; 4],
            gate_length: [127; 4],
            range: [3; 4],
            oct: [0; 4],
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
    let (midi_chan1, midi_chan2, midi_chan3, midi_chan4) = params.query(|p| {
        (
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
        app.use_midi_output(midi_chan1 as u8 - 1),
        app.use_midi_output(midi_chan2 as u8 - 1),
        app.use_midi_output(midi_chan3 as u8 - 1),
        app.use_midi_output(midi_chan4 as u8 - 1),
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
    let length_flag: Global<bool> = app.make_global(false);
    let seq_glob: Global<[u16; 64]> = app.make_global([0; 64]);
    let gateseq_glob: Global<[bool; 64]> = app.make_global([true; 64]);
    let legatoseq_glob: Global<[bool; 64]> = app.make_global([false; 64]);

    let seq_length_glob: Global<[u8; 4]> = app.make_global([16; 4]);
    let gatelength_glob: Global<[u8; 4]> = app.make_global([128; 4]);

    let clockres_glob = app.make_global([6, 6, 6, 6]);

    let resolution = [24, 16, 12, 8, 6, 4, 3, 2];

    let mut lastnote = [0; 4];
    let mut gatelength1 = gatelength_glob.get();

    let (seq_saved, gateseq_saved, seq_length_saved, mut clockres, mut gatel, legato_seq_saved) =
        storage.query(|s| {
            (
                s.seq,
                s.gateseq,
                s.seq_length,
                s.seqres,
                s.gate_length,
                s.legato_seq,
            )
        });

    seq_glob.set(seq_saved.get());
    gateseq_glob.set(gateseq_saved.get());
    seq_length_glob.set(seq_length_saved);
    legatoseq_glob.set(legato_seq_saved.get());

    for n in 0..4 {
        clockres[n] = resolution[clockres[n]];
        gatel[n] = (clockres[n] * gatel[n] as usize / 256) as u8;
        gatel[n] = gatel[n].clamp(1, clockres[n] as u8 - 1);
    }
    clockres_glob.set(clockres);
    gatelength_glob.set(gatel);

    let fader_fut = async {
        let mut fader_values = faders.get_all_values();
        let mut latches = [AnalogLatch::new(0); 8];
        for (i, latch) in latches.iter_mut().enumerate() {
            *latch = AnalogLatch::new(fader_values[i]);
        }

        loop {
            let chan = faders.wait_for_any_change().await;
            // Update local cache of values
            fader_values = faders.get_all_values();
            let val = fader_values[chan];

            let page = page_glob.get();
            let shift = buttons.is_shift_pressed();
            let layer = LatchLayer::from(shift);

            // Calculate target and perform update
            let target = if !shift {
                // Main Layer: Sequence Steps
                // Target is the stored value
                seq_glob.get()[chan + (page * 8)]
            } else {
                // Alt Layer: Parameters
                // We calculate a "virtual" target. If the fader is currently in the zone
                // of the parameter we set the target to the fader's value (latch immediately).
                // Otherwise, we set it to the start of the zone (pick up).
                match chan {
                    0 => {
                        // Seq Length (1..16 mapped to 0..4095)
                        let current_param = seq_length_glob.get()[page / 2];
                        let fader_param = (val / 256 + 1) as u8;
                        if fader_param == current_param {
                            val
                        } else {
                            (current_param as u16).saturating_sub(1) * 256
                        }
                    }
                    1 => {
                        // Gate Length
                        let current_param = storage.query(|s| s.gate_length[page / 2]);
                        let fader_param = (val / 16) as u8;
                        // Use tolerance check from original code logic (approx)
                        if (fader_param as i16 - current_param as i16).abs() < 6 {
                            val
                        } else {
                            current_param as u16 * 16
                        }
                    }
                    2 => {
                        // Octave
                        let current_param = storage.query(|s| s.oct[page / 2]);
                        let fader_param = (val / 1000) as u8;
                        if fader_param == current_param {
                            val
                        } else {
                            current_param as u16 * 1000
                        }
                    }
                    3 => {
                        // Range
                        let current_param = storage.query(|s| s.range[page / 2]);
                        let fader_param = (val / 1000 + 1) as u8;
                        if fader_param == current_param {
                            val
                        } else {
                            (current_param as u16).saturating_sub(1) * 1000
                        }
                    }
                    4 => {
                        // Resolution
                        let current_param = storage.query(|s| s.seqres[page / 2]) as u16;
                        let fader_param = val / 512;
                        if fader_param == current_param {
                            val
                        } else {
                            current_param * 512
                        }
                    }
                    _ => 0,
                }
            };

            if let Some(new_val) = latches[chan].update(val, layer, target) {
                if !shift {
                    let mut seq = seq_glob.get();
                    seq[chan + (page * 8)] = new_val;
                    seq_glob.set(seq);
                    storage.modify_and_save(|s| s.seq.set(seq));
                } else {
                    // Apply parameter changes
                    match chan {
                        0 => {
                            let mut seq_length = seq_length_glob.get();
                            let new_len = (new_val / 256 + 1) as u8;
                            if seq_length[page / 2] != new_len {
                                seq_length[page / 2] = new_len;
                                seq_length_glob.set(seq_length);
                                storage.modify_and_save(|s| s.seq_length = seq_length);
                                length_flag.set(true);
                            }
                        }
                        1 => {
                            let new_len = (new_val / 16) as u8;
                            storage.modify_and_save(|s| s.gate_length[page / 2] = new_len);

                            let mut gatelength = gatelength_glob.get();
                            let clockres = clockres_glob.get();
                            let mut calc = (clockres[page / 2] * (new_val as usize) / 4096) as u8;
                            calc = calc.clamp(1, clockres[page / 2] as u8 - 1);
                            gatelength[page / 2] = calc;
                            gatelength_glob.set(gatelength);
                        }
                        2 => {
                            let new_oct = (new_val / 1000) as u8;
                            storage.modify_and_save(|s| s.oct[page / 2] = new_oct);
                        }
                        3 => {
                            let new_range = (new_val / 1000 + 1) as u8;
                            storage.modify_and_save(|s| s.range[page / 2] = new_range);
                        }
                        4 => {
                            let new_res_idx = (new_val / 512) as usize;
                            if new_res_idx < resolution.len() {
                                storage.modify_and_save(|s| s.seqres[page / 2] = new_res_idx);

                                let mut clockres = clockres_glob.get();
                                clockres[page / 2] = resolution[new_res_idx];
                                clockres_glob.set(clockres);

                                let mut gatelength = gatelength_glob.get();
                                gatelength[page / 2] =
                                    gatelength[page / 2].clamp(1, clockres[page / 2] as u8);
                                gatelength_glob.set(gatelength);
                            }
                        }
                        _ => {}
                    }
                }
            }
            led_flag_glob.set(true);
        }
    };

    let button_fut = async {
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

                led_flag_glob.set(true);
            } else {
                page_glob.set(chan);
            }
        }
    };

    let long_press_fut = async {
        loop {
            let (chan, is_shift_pressed) = buttons.wait_for_any_long_press().await;

            // let mut gateseq = gateseq_glob.get_array();
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
            }
        }
    };

    let led_fut = async {
        loop {
            let intensities = [
                Brightness::Lowest,
                Brightness::Lower,
                Brightness::Low,
                Brightness::Default,
            ];
            let colors = [Color::Yellow, Color::Pink, Color::Cyan, Color::White];
            app.delay_millis(16).await;
            let clockres = clockres_glob.get();

            if buttons.is_shift_pressed() {
                let clockn = clockn_glob.get();

                let seq_length = seq_length_glob.get();

                let page = page_glob.get();
                let mut bright = Brightness::Lower;
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
                        bright = Brightness::Lower;
                    }
                    if n == (clockn / clockres[page / 2]) as u8 % seq_length[page / 2] {
                        bright = Brightness::Default;
                    }
                    if n >= seq_length[page / 2] {
                        bright = Brightness::Custom(0);
                    }
                    if n < 8 {
                        led.set(n as usize, Led::Top, Color::Red, bright)
                    } else {
                        led.set(n as usize - 8, Led::Bottom, Color::Red, bright)
                    }
                }
            }

            if !buttons.is_shift_pressed() {
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
                        // TODO: this needs changing
                        led.set(n, Led::Bottom, Color::Red, Brightness::Lower)
                    } else {
                        led.unset(n, Led::Bottom);
                    }
                }
                // runing light on buttons
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
                        Brightness::Lower,
                    );
                }

                led.set(page, Led::Bottom, color, Brightness::Default);
            }

            led_flag_glob.set(false);
        }
    };

    let seq_fut = async {
        loop {
            let gateseq = gateseq_glob.get();
            let seq_length = seq_length_glob.get();
            let clockres = clockres_glob.get();
            let legato_seq = legatoseq_glob.get();

            let mut clockn = clockn_glob.get();

            match clk.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset => {
                    clockn = 0;
                    // info!("reset!");
                    for n in 0..4 {
                        midi[n].send_note_off(lastnote[n]).await;
                        gate_out[n].set_low().await;
                    }
                }
                ClockEvent::Tick => {
                    for n in 0..=3 {
                        if clockn % clockres[n] == 0 {
                            let clkindex =
                                (clockn / clockres[n] % seq_length[n] as usize) + (n * 16);
                            midi[n].send_note_off(lastnote[n]).await;
                            if gateseq[clkindex] {
                                let seq = seq_glob.get();

                                let out = quantizer
                                    .get_quantized_note(
                                        (seq[clkindex] as u32
                                            * (storage.query(|s| s.range[n]) as u32)
                                            * 410
                                            / 4095) as u16
                                            + storage.query(|s| s.oct[n]) as u16 * 410,
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
                        if (clockn - gatelength1[n] as usize) % clockres[n] == 0 {
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
                SceneEvent::LoadSscene(scene) => {
                    storage.load_from_scene(scene).await;

                    let (
                        seq_saved,
                        gateseq_saved,
                        seq_length_saved,
                        mut clockres,
                        mut gatel,
                        legato_seq_saved,
                    ) = storage.query(|s| {
                        (
                            s.seq,
                            s.gateseq,
                            s.seq_length,
                            s.seqres,
                            s.gate_length,
                            s.legato_seq,
                        )
                    });

                    seq_glob.set(seq_saved.get());
                    gateseq_glob.set(gateseq_saved.get());
                    seq_length_glob.set(seq_length_saved);
                    legatoseq_glob.set(legato_seq_saved.get());

                    for n in 0..4 {
                        clockres[n] = resolution[clockres[n]];
                        gatel[n] = (clockres[n] * gatel[n] as usize / 256) as u8;
                        gatel[n] = gatel[n].clamp(1, clockres[n] as u8 - 1);
                    }
                    clockres_glob.set(clockres);
                    gatelength_glob.set(gatel);
                }
                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    join(
        join5(fader_fut, button_fut, long_press_fut, led_fut, seq_fut),
        scene_handler,
    )
    .await;
}
