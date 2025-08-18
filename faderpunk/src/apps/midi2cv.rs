use defmt::info;
use embassy_futures::{
    join::{join4, join5},
    select::select,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use libfp::{
    constants::{ATOV_PURPLE, ATOV_RED, LED_MID},
    utils::{
        attenuate_bipolar, bits_7_16, clickless, is_close, scale_bits_7_12, split_unsigned_value,
    },
};
use midly::MidiMessage;
use serde::{Deserialize, Serialize};

use libfp::{
    quantizer::{self, Key, Note},
    Config, Curve, Param, Value,
};

use crate::app::{
    App, AppStorage, Led, ManagedStorage, MidiReceiver, MidiSender, ParamSlot, ParamStore, Range,
    SceneEvent, RGB8,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 4;

pub static CONFIG: Config<PARAMS> = Config::new("MIDI2CV", "Multifunctional MIDI to CV")
    .add_param(Param::i32 {
        name: "Mode, 0 = cc, 1 = pitch, 2 = gate, 3 = vel, 4 = atouch",
        min: 0,
        max: 4,
    })
    .add_param(Param::Curve {
        name: "Curve",
        variants: &[Curve::Linear, Curve::Exponential, Curve::Logarithmic],
    })
    // .add_param(Param::Bool { name: "Bipolar" })
    .add_param(Param::i32 {
        name: "MIDI Channel",
        min: 1,
        max: 16,
    })
    .add_param(Param::i32 {
        name: "MIDI CC",
        min: 1,
        max: 128,
    });

const LED_COLOR: RGB8 = ATOV_PURPLE;
const BUTTON_BRIGHTNESS: u8 = LED_MID;

// TODO: Make a macro to generate this.
#[derive(Serialize, Deserialize)]
pub struct Storage {
    muted: bool,
    att_saved: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            muted: false,
            att_saved: 4095,
        }
    }
}

impl AppStorage for Storage {}

// TODO: Make a macro to generate this.
pub struct Params<'a> {
    mode: ParamSlot<'a, i32, PARAMS>,
    curve: ParamSlot<'a, Curve, PARAMS>,
    // bipolar: ParamSlot<'a, bool, PARAMS>,
    midi_channel: ParamSlot<'a, i32, PARAMS>,
    midi_cc: ParamSlot<'a, i32, PARAMS>,
}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    // TODO: Make a macro to generate this.
    // TODO: Move Signal (when changed) to store so that we can do params.wait_for_change maybe
    // TODO: Generate this from the static params defined above
    let param_store = ParamStore::new(
        [
            Value::i32(0),
            Value::Curve(Curve::Linear),
            // Value::bool(false),
            Value::i32(1),
            Value::i32(32),
        ],
        app.app_id,
        app.start_channel,
    );

    let params = Params {
        mode: ParamSlot::new(&param_store, 0),
        curve: ParamSlot::new(&param_store, 1),
        // bipolar: ParamSlot::new(&param_store, 1),
        midi_channel: ParamSlot::new(&param_store, 2),
        midi_cc: ParamSlot::new(&param_store, 3),
    };

    let app_loop = async {
        loop {
            let storage = ManagedStorage::<Storage>::new(app.app_id, app.start_channel);
            param_store.load().await;
            storage.load(None).await;
            select(run(&app, &params, storage), param_store.param_handler()).await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(app: &App<CHANNELS>, params: &Params<'_>, storage: ManagedStorage<Storage>) {
    let buttons = app.use_buttons();
    let fader = app.use_faders();
    let leds = app.use_leds();

    let midi_chan = params.midi_channel.get().await;
    let midi_cc = params.midi_cc.get().await;
    let curve = params.curve.get().await;
    let midi_in = app.use_midi_input(midi_chan as u8 - 1);

    let muted_glob = app.make_global(false);
    let att_glob = app.make_global(4095);
    let latched_glob = app.make_global(false);
    let offset_glob = app.make_global(0);

    let muted = storage.query(|s| s.muted).await;
    let att = storage.query(|s| s.att_saved).await;
    muted_glob.set(muted).await;
    att_glob.set(att).await;

    let mut quantizer = app.use_quantizer();

    let mode = params.mode.get().await;

    leds.set(
        0,
        Led::Button,
        LED_COLOR,
        if muted { 0 } else { BUTTON_BRIGHTNESS },
    );

    // let jack = if !params.bipolar.get().await {
    //     app.make_out_jack(0, Range::_0_10V).await
    // } else {
    //     app.make_out_jack(0, Range::_Neg5_5V).await
    // };

    let jack = app.make_out_jack(0, Range::_0_10V).await;
    jack.set_value(0);

    let fut1 = async {
        let mut outval = 0.;
        let mut val = fader.get_value();
        let mut fadval = fader.get_value();
        let mut old_midi = 0;
        let mut attval = 0;
        let mut shift_old = false;

        loop {
            app.delay_millis(1).await;
            if mode == 0 {
                let muted = muted_glob.get().await;
                if !buttons.is_shift_pressed() {
                    fadval = fader.get_value();
                }
                let att = att_glob.get().await;
                let offset = offset_glob.get().await;

                // if buttons.is_shift_pressed() {

                if muted {
                    val = 0;
                } else {
                    val = curve.at((fadval + offset).into());
                }
                if buttons.is_shift_pressed() {
                    leds.set(0, Led::Top, ATOV_RED, (att / 16) as u8);
                    leds.set(0, Led::Bottom, ATOV_RED, 0);
                } else {
                    leds.set(0, Led::Top, LED_COLOR, (outval / 16.) as u8);
                }
                outval = clickless(outval, val);
                attval = ((outval as u32 * att as u32) / 4095) as u16;

                jack.set_value(attval);

                if !shift_old && buttons.is_shift_pressed() {
                    latched_glob.set(false).await;
                    shift_old = true;
                }
                if shift_old && !buttons.is_shift_pressed() {
                    latched_glob.set(false).await;

                    shift_old = false;
                }
            }
        }
    };

    let fut2 = async {
        loop {
            buttons.wait_for_down(0).await;

            let muted = storage
                .modify_and_save(
                    |s| {
                        s.muted = !s.muted;
                        s.muted
                    },
                    None,
                )
                .await;
            muted_glob.set(muted).await;
            if muted {
                leds.reset(0, Led::Button);
            } else {
                leds.set(0, Led::Button, LED_COLOR, 100);
            }
        }
    };
    let fut3 = async {
        loop {
            fader.wait_for_change().await;
            let fader_val = fader.get_value();

            if !latched_glob.get().await && is_close(fader_val, att_glob.get().await) {
                latched_glob.set(true).await;
            }
            if buttons.is_shift_pressed() && latched_glob.get().await {
                att_glob.set(fader_val).await;
                storage
                    .modify_and_save(
                        |s| {
                            s.att_saved = fader_val;
                            s.att_saved
                        },
                        None,
                    )
                    .await;
            }
        }
    };

    let fut4 = async {
        let mut note_num = 0;
        loop {
            match midi_in.wait_for_message().await {
                MidiMessage::Controller { controller, value } => {
                    if mode == 0 {
                        if bits_7_16(controller) == params.midi_cc.get().await as u16 {
                            let val = scale_bits_7_12(value);
                            offset_glob.set(val).await
                        }
                    }
                }
                MidiMessage::NoteOn { key, vel } => {
                    if mode == 1 {
                        let mut note_in = bits_7_16(key);
                        note_in = (note_in as u32 * 410 / 12) as u16;
                        let oct = (fader.get_value() as i32 * 10 / 4095) - 5;
                        let note_out = (note_in as i32 + oct * 410).clamp(0, 4095) as u16;
                        jack.set_value(note_out);
                        leds.set(0, Led::Top, LED_COLOR, (note_out / 16) as u8);
                        info!("{}", note_out)
                    }
                    if mode == 2 {
                        jack.set_value(4095);
                        note_num += 1;
                        leds.set(0, Led::Top, LED_COLOR, LED_MID);
                        info!("note on num = {}", note_num);
                    }
                    if mode == 3 {
                        let vel_out = scale_bits_7_12(vel);
                        jack.set_value(vel_out);
                        leds.set(0, Led::Top, LED_COLOR, (vel_out / 16) as u8);
                        info!("Velocity: {} ", vel_out)
                    }
                }
                MidiMessage::NoteOff { key, vel } => {
                    if mode == 2 {
                        note_num = (note_num - 1).max(0);
                        info!("note off num = {}", note_num);
                        if note_num == 0 {
                            jack.set_value(0);
                            leds.set(0, Led::Top, LED_COLOR, 0);
                        }
                    }
                }
                MidiMessage::Aftertouch { key, vel } => {
                    info!("aftertouch");
                    if mode == 4 {
                        let vel_out = scale_bits_7_12(key);
                        jack.set_value(vel_out);
                    }
                }
                MidiMessage::PitchBend { bend } => {
                    info!("Bend!")
                }

                _ => {}
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load(Some(scene)).await;
                    let muted = storage.query(|s| s.muted).await;
                    muted_glob.set(muted).await;
                    if muted {
                        leds.reset(0, Led::Button);
                    } else {
                        leds.set(0, Led::Button, LED_COLOR, 100);
                    }
                }
                SceneEvent::SaveScene(scene) => storage.save(Some(scene)).await,
            }
        }
    };

    join5(fut1, fut2, fut3, fut4, scene_handler).await;
}
