// Todo :
// Save div, mute, attenuation - Added the saving slots, need to add write/read in the app.
// Add attenuator (shift + fader)

use embassy_futures::{join::join5, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use serde::{Deserialize, Serialize};

use crate::app::{
    App, AppStorage, ClockEvent, Led, ManagedStorage, ParamSlot, ParamStore, SceneEvent, RGB8,
};

use libfp::{
    colors::PURPLE,
    utils::{attenuate, attenuate_bipolar, is_close, split_unsigned_value},
    Brightness, Config, Param, Range, Value,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 2;

const LED_COLOR: RGB8 = PURPLE;

pub static CONFIG: Config<PARAMS> = Config::new("Random CC/CV", "Generate random values on clock")
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

pub struct Params<'a> {
    midi_channel: ParamSlot<'a, i32, PARAMS>,
    cc: ParamSlot<'a, i32, PARAMS>,
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    fader_saved: u16,
    mute_save: bool,
    att_saved: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            fader_saved: 3000,
            mute_save: false,
            att_saved: 4096,
        }
    }
}
impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::new(
        [Value::i32(1), Value::i32(32)],
        app.app_id,
        app.start_channel,
    );

    let params = Params {
        midi_channel: ParamSlot::new(&param_store, 0),
        cc: ParamSlot::new(&param_store, 1),
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
    let midi_chan = params.midi_channel.get().await;
    let cc = params.cc.get().await;

    let mut clock = app.use_clock();
    let rnd = app.use_die();
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let midi = app.use_midi_output(midi_chan as u8 - 1);
    let output = app.make_out_jack(0, Range::_Neg5_5V).await;

    let glob_muted = app.make_global(false);
    let div_glob = app.make_global(6);
    let att_glob = app.make_global(4096);
    let latched_glob = app.make_global(false);

    let resolution = [368, 184, 92, 48, 24, 16, 12, 8, 6, 4, 3, 2];

    let mut clkn = 0;
    let mut val = 2048;

    let (res, mute, att) = storage.query(|s| (s.fader_saved, s.mute_save, s.att_saved));

    att_glob.set(att);
    glob_muted.set(mute);
    div_glob.set(resolution[res as usize / 345]);
    if mute {
        leds.unset(0, Led::Button);
        output.set_value(2047);
        midi.send_cc(cc as u8, 0).await;
        leds.unset(0, Led::Top);
        leds.unset(0, Led::Bottom);
    } else {
        leds.set(0, Led::Button, LED_COLOR, Brightness::Lower);
    }

    let fut1 = async {
        loop {
            match clock.wait_for_event(1).await {
                ClockEvent::Reset => {
                    clkn = 0;
                }
                ClockEvent::Tick => {
                    let muted = glob_muted.get();
                    let att = att_glob.get();
                    let div = div_glob.get();
                    if clkn % div == 0 && !muted {
                        let midival = attenuate(val, att);
                        let jackval = attenuate_bipolar(val, att);
                        output.set_value(jackval);
                        midi.send_cc(cc as u8, midival).await;
                        let ledj = split_unsigned_value(jackval);
                        let r = (rnd.roll() / 16) as u8;
                        let g = (rnd.roll() / 16) as u8;
                        let b = (rnd.roll() / 16) as u8;

                        let color: RGB8 = RGB8 { r, g, b };
                        leds.set(0, Led::Top, color, Brightness::Custom(ledj[0]));
                        leds.set(0, Led::Bottom, color, Brightness::Custom(ledj[1]));
                        leds.set(0, Led::Button, color, Brightness::Lower);
                        val = rnd.roll();
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
            let muted = glob_muted.toggle();

            storage
                .modify_and_save(
                    |s| {
                        s.mute_save = muted;
                        s.mute_save
                    },
                    None,
                )
                .await;

            if muted {
                output.set_value(2047);
                midi.send_cc(cc as u8, 0).await;
                leds.unset_all();
            } else {
                leds.set(0, Led::Button, LED_COLOR, Brightness::Lower);
            }
        }
    };

    let fut3 = async {
        loop {
            fader.wait_for_change_at(0).await;
            storage.load(None).await;
            let fad = fader.get_value();

            if !buttons.is_shift_pressed() {
                let fad_saved = storage.query(|s| s.fader_saved);
                if is_close(fad, fad_saved) {
                    latched_glob.set(true);
                }
                if latched_glob.get() {
                    div_glob.set(resolution[fad as usize / 345]);
                    storage.modify_and_save(|s| s.fader_saved = fad, None).await;
                }
            } else {
                let att = att_glob.get();
                if is_close(fad, att) {
                    latched_glob.set(true);
                }
                if latched_glob.get() {
                    att_glob.set(fad);
                    storage.modify_and_save(|s| s.att_saved = fad, None).await;
                }
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load(Some(scene)).await;
                    let (res, mute, att) =
                        storage.query(|s| (s.fader_saved, s.mute_save, s.att_saved));

                    att_glob.set(att);
                    glob_muted.set(mute);
                    div_glob.set(resolution[res as usize / 345]);
                    if mute {
                        leds.set(0, Led::Button, LED_COLOR, Brightness::Lower);
                        output.set_value(2047);
                        midi.send_cc(cc as u8, 0).await;
                        leds.unset(0, Led::Top);
                        leds.unset(0, Led::Bottom);
                    }
                    latched_glob.set(false);
                }

                SceneEvent::SaveScene(scene) => {
                    storage.save(Some(scene)).await;
                }
            }
        }
    };

    let mut shift_old = false;

    let shift = async {
        loop {
            // latching on pressing and depressing shift

            app.delay_millis(1).await;
            if !shift_old && buttons.is_shift_pressed() {
                latched_glob.set(false);
                shift_old = true;
            }
            if shift_old && !buttons.is_shift_pressed() {
                latched_glob.set(false);
                shift_old = false;
            }
        }
    };

    join5(fut1, fut2, fut3, scene_handler, shift).await;
}
