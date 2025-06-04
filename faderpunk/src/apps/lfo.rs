use embassy_futures::join::{join3, join4};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use crate::app::{App, Led, Range, SceneEvent};
use config::Waveform;
use libfp::constants::CURVE_LOG;
use serde::{Deserialize, Serialize};

pub const CHANNELS: usize = 1;

// app_config! (
//     config("LFO", "Wooooosh");
//     params();
//     storage();
// );

#[derive(Serialize, Deserialize, Default)]
pub struct Storage { //waveform and frequency
    glob_lfo_speed: f32,
    glob_wave: Waveform,
}

#[embassy_executor::task(pool_size = 16)]
pub async fn run(app: App<CHANNELS>) {
    //let glob_wave = app.make_global(Waveform::Sine);
    //let glob_lfo_speed = app.make_global(0.0682);
    let glob_lfo_pos = app.make_global(0.0);

    let output = app.make_out_jack(0, Range::_Neg5_5V).await;
    let faders = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    
    let storage: Mutex<NoopRawMutex, Storage> = Mutex::new(
        app.load::<Storage>(None)
            .await
            .unwrap_or(Storage::default()),
    );

    let glob_lfo_speed = {
        let stor = storage.lock().await;
        stor.glob_lfo_speed
    };
    let glob_wave = {
        let stor = storage.lock().await;
        stor.glob_wave
    };

    

    let fut1 = async {
        loop {
            app.delay_millis(1).await;
            let glob_lfo_speed = {
                let stor = storage.lock().await;
                stor.glob_lfo_speed
            };

            let glob_wave = {
                let stor = storage.lock().await;
                stor.glob_wave
            };


            let wave = glob_wave;
            let lfo_speed = glob_lfo_speed;
            let lfo_pos = glob_lfo_pos.get().await;
            let next_pos = (lfo_pos + lfo_speed) % 4096.0;

            let val = wave.at(next_pos as usize);

            output.set_value(val);

            let color = match wave {
                Waveform::Sine => (243, 191, 78),
                Waveform::Triangle => (188, 77, 216),
                Waveform::Saw => (78, 243, 243),
                Waveform::Rect => (250, 250, 250),
            };

            leds.set(0, Led::Button, color, 75); //75 is good for shooting
            leds.set(0, Led::Top, color, ((val as f32 / 16.0) / 2.0) as u8);
            leds.set(
                0,
                Led::Bottom,
                color,
                ((255.0 - (val as f32) / 16.0) / 2.0) as u8,
            );
            glob_lfo_pos.set(next_pos).await;
        }
    };

    let fut2 = async {
        loop {
            faders.wait_for_change(0).await;
            let [fader] = faders.get_values();

            let mut stor = storage.lock().await;
            stor.glob_lfo_speed = CURVE_LOG[fader as usize] as f32 * 0.015 + 0.0682;
            app.save(&*stor, None).await;
        }
    };

    let fut3 = async {
        loop {
            buttons.wait_for_down(0).await;
            
            let mut wave = {
                let stor = storage.lock().await;
                stor.glob_wave
            };
            wave = wave.cycle();
            let mut stor = storage.lock().await;
            stor.glob_wave = wave;
            app.save(&*stor, None).await;
        }
    };

        let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    defmt::info!("LOADING SCENE {}", scene);
                    let mut stor = storage.lock().await;
                    let scene_stor = app
                        .load::<Storage>(Some(scene))
                        .await
                        .unwrap_or(Storage::default());
                    *stor = scene_stor;
                    
                    //update_outputs(stor.glob_lfo_speed).await;
                }
                SceneEvent::SaveScene(scene) => {
                    defmt::info!("SAVING SCENE {}", scene);
                    let stor = storage.lock().await;
                    app.save(&*stor, Some(scene)).await;
                }
            }
        }
    };

    join4(fut1, fut2, fut3, scene_handler).await;
}
