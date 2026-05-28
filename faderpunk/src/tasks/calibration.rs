use defmt::info;
use embassy_futures::select::{select, select3, Either, Either3};
use embassy_time::Timer;
use linreg::linear_regression;
use max11300::config::{ConfigMode5, ConfigMode7, Mode, Port, ADCRANGE, AVR, DACRANGE, NSAMPLES};
use portable_atomic::Ordering;

use libfp::{
    types::{MaxCalibration, RegressionValuesInput, RegressionValuesOutput},
    Brightness, Color, CALIBRATION_SCALE_FACTOR,
};

use crate::app::Led;
use crate::events::{InputEvent, EVENT_PUBSUB};
use crate::storage::store_calibration_data;
use crate::tasks::buttons::BUTTON_PRESSED;
use crate::tasks::i2c::{I2cFollowerMessage, I2cFollowerReceiver};
use crate::tasks::leds::{set_led_mode, LedMode, LedMsg};
use crate::tasks::max::{MaxCmd, CALIBRATING, MAX_CHANNEL, MAX_VALUES_ADC, MAX_VALUES_FADER};

use super::max::MAX_VALUES_DAC;

const CHANNELS: usize = 16;
const VALUES_OUT_0_10V: [u16; 3] = [819, 1638, 3276];
const VOLTAGES_OUT_0_10V: [f32; 3] = [2.0, 4.0, 8.0];
const VALUES_IN_0_10V: [u16; 3] = [0, 819, 4095];
const VOLTAGES_IN_0_10V: [f32; 3] = [0.0, 2.0, 10.0];
const VALUES_NEG5_5V: [u16; 3] = [819, 2048, 3276];
const VOLTAGES_NEG5_5V: [f32; 3] = [-3.0, 0.0, 3.0];
const LED_POS: [Led; 3] = [Led::Button, Led::Bottom, Led::Top];

fn set_led_color(ch: usize, pos: Led, color: Color) {
    set_led_mode(
        ch,
        pos,
        LedMsg::Set(LedMode::Static(color, Brightness::High)),
    );
}

fn reset_led(ch: usize, pos: Led) {
    set_led_mode(ch, pos, LedMsg::Reset);
}

fn flash_led(ch: usize, pos: Led, color: Color, times: Option<usize>) {
    set_led_mode(ch, pos, LedMsg::Set(LedMode::Flash(color, times)));
}

async fn wait_for_button_press(channel: usize) -> bool {
    let mut input_subscriber = EVENT_PUBSUB.subscriber().unwrap();
    loop {
        if let InputEvent::ButtonDown(idx) = input_subscriber.next_message_pure().await {
            if idx == channel {
                return BUTTON_PRESSED[17].load(Ordering::Relaxed);
            }
        }
    }
}

async fn wait_for_start_cmd(msg_receiver: &mut I2cFollowerReceiver) {
    loop {
        if let I2cFollowerMessage::CalibStart = msg_receiver.receive().await {
            return;
        }
    }
}

async fn configure_jack(ch: usize, mode: Mode) {
    let port = Port::try_from(ch).unwrap();
    MAX_CHANNEL
        .sender()
        .send(MaxCmd::ConfigurePort {
            port,
            mode,
            gpo_level: None,
        })
        .await;
}

async fn run_manual_input_calibration() -> RegressionValuesInput {
    let mut input_results = RegressionValuesInput::default();
    let adc_ranges = [ADCRANGE::Rg0_10v, ADCRANGE::RgNeg5_5v];
    let voltages_arrays = [VOLTAGES_IN_0_10V, VOLTAGES_NEG5_5V];
    let values_arrays = [VALUES_IN_0_10V, VALUES_NEG5_5V];
    set_led_color(0, Led::Button, Color::Cyan);
    info!("Plug a good voltage source into channel 0, then press button");
    wait_for_button_press(0).await;
    for (range_index, &adc_range) in adc_ranges.iter().enumerate() {
        let mut measured_values: [u16; 3] = Default::default();
        let target_values = values_arrays[range_index];

        for (j, (&voltage, &target_value)) in voltages_arrays[range_index]
            .iter()
            .zip(target_values.iter())
            .enumerate()
        {
            // Configure first channel to be input
            configure_jack(
                0,
                Mode::Mode7(ConfigMode7(
                    AVR::InternalRef,
                    adc_range,
                    NSAMPLES::Samples16,
                )),
            )
            .await;
            let pos = LED_POS[j];
            flash_led(0, pos, Color::Cyan, None);
            info!("Set voltage source to {}V, then press button", voltage);
            wait_for_button_press(0).await;
            let value = MAX_VALUES_ADC[0].load(Ordering::Relaxed);
            measured_values[j] = value;
            set_led_color(0, pos, Color::Cyan);
            let error = target_value as i16 - value as i16;
            info!("Target value: {}", target_value);
            info!("Value read: {}", value);
            info!("Error: {}", error);
            info!("------------------");
        }

        if let Ok(results) = linear_regression::<f32, f32, f32>(
            &measured_values.map(|v| v as f32),
            &target_values.map(|v| v as f32),
        ) {
            // Convert f32 results to i64 fixed-point format
            let slope = (results.0 * CALIBRATION_SCALE_FACTOR as f32) as i64;
            let intercept = (results.1 * CALIBRATION_SCALE_FACTOR as f32) as i64;
            input_results[range_index] = (slope, intercept);
            info!(
                "Linear regression results for range {}: {}",
                range_index,
                (slope, intercept)
            );
        } else {
            // Blink LED red if calibration didn't succeeed
            flash_led(0, Led::Button, Color::Red, None);
            loop {
                Timer::after_secs(10).await;
            }
        }
    }

    input_results
}

async fn run_manual_output_calibration() -> RegressionValuesOutput {
    let mut output_results = RegressionValuesOutput::default();

    for i in 0..CHANNELS {
        for &p in LED_POS.iter() {
            reset_led(i, p);
        }
    }

    info!("Remove voltage source NOW, then press button");
    wait_for_button_press(0).await;

    let dac_ranges = [DACRANGE::Rg0_10v, DACRANGE::RgNeg5_5v];
    let voltages_arrays = [VOLTAGES_OUT_0_10V, VOLTAGES_NEG5_5V];
    let values_arrays = [VALUES_OUT_0_10V, VALUES_NEG5_5V];

    // Bottom LED = range 0 (0-10V) progress, Top LED = range 1 (±5V) progress
    const RANGE_LED: [Led; 2] = [Led::Bottom, Led::Top];

    let channels_to_calibrate: [usize; 19] =
        core::array::from_fn(|i| if i < 16 { i } else { i + 1 });
    let mut i = 0;
    'channel_loop: while i < channels_to_calibrate.len() {
        let chan = channels_to_calibrate[i];
        let ui_no = chan % 17;
        let prev_ui_no = (ui_no + CHANNELS - 1) % CHANNELS;

        // Yellow button = this channel is being calibrated
        set_led_color(ui_no, Led::Button, Color::Yellow);
        reset_led(ui_no, Led::Bottom);
        reset_led(ui_no, Led::Top);

        for (range_idx, &dac_range) in dac_ranges.iter().enumerate() {
            let mut set_values: [u16; 3] = Default::default();
            let target_values = values_arrays[range_idx];

            let port = Port::try_from(chan).unwrap();
            MAX_CHANNEL
                .send(MaxCmd::ConfigurePort {
                    port,
                    mode: Mode::Mode5(ConfigMode5(dac_range)),
                    gpo_level: None,
                })
                .await;

            info!("Calibrating DAC range index: {}", range_idx);

            for (j, (&voltage, &target_value)) in voltages_arrays[range_idx]
                .iter()
                .zip(target_values.iter())
                .enumerate()
            {
                let pos = RANGE_LED[range_idx];
                flash_led(ui_no, pos, Color::Green, None);
                info!(
                    "Move fader {} until you read the closest value to {}V, then press button",
                    ui_no, voltage
                );
                let mut value = 0;

                'step_loop: loop {
                    // Re-create the future on every iteration to avoid the move error
                    let loop1 = async {
                        loop {
                            Timer::after_millis(10).await;
                            let offset = ((MAX_VALUES_FADER[ui_no].load(Ordering::Relaxed) as f32)
                                / 152.0) as u16;
                            let base = target_value - 13;
                            value = base + offset;
                            MAX_VALUES_DAC[chan].store(value, Ordering::Relaxed);
                        }
                    };

                    let wait_next = wait_for_button_press(ui_no);

                    if i == 0 {
                        select(loop1, wait_next).await;
                        // `select` returns when `wait_next` completes, so we can proceed.
                        break 'step_loop;
                    } else {
                        let wait_prev = wait_for_button_press(prev_ui_no);
                        match select3(loop1, wait_next, wait_prev).await {
                            Either3::First(_) => {
                                // This is an infinite loop, so it will never complete
                            }
                            Either3::Second(_) => {
                                // "next" was pressed, proceed to process the value
                                break 'step_loop;
                            }
                            Either3::Third(is_shift_pressed) => {
                                // "prev" button was pressed
                                if is_shift_pressed {
                                    i -= 1;
                                    // Reset all LEDs for the channel we are leaving
                                    for &p in LED_POS.iter() {
                                        reset_led(ui_no, p);
                                    }
                                    continue 'channel_loop;
                                } else {
                                    // Prev without shift, so ignore and re-wait.
                                    continue 'step_loop;
                                }
                            }
                        }
                    }
                }

                set_led_color(ui_no, RANGE_LED[range_idx], Color::Green);
                set_values[j] = value;
                let error = target_value as i16 - value as i16;
                info!("Target value: {}", target_value);
                info!("Read value: {}", value);
                info!("Error: {} counts", error);
                info!("------------------");
            }

            if let Ok(results) = linear_regression::<f32, f32, f32>(
                &set_values.map(|v| v as f32),
                &target_values.map(|v| v as f32),
            ) {
                // Convert f32 results to i64 fixed-point format
                let slope = (results.0 * CALIBRATION_SCALE_FACTOR as f32) as i64;
                let intercept = (results.1 * CALIBRATION_SCALE_FACTOR as f32) as i64;
                output_results[chan][range_idx] = (slope, intercept);
                info!(
                    "Linear regression results for outputs channel {} range {}: ({}, {})",
                    chan, range_idx, slope, intercept
                );
            } else {
                // Blink LED red if calibration didn't succeeed
                flash_led(ui_no, Led::Button, Color::Red, None);
                loop {
                    Timer::after_secs(10).await;
                }
            }
        }

        // Both ranges done — show green button for this channel
        set_led_color(ui_no, Led::Button, Color::Green);

        if chan == 15 {
            for chan in 0..CHANNELS {
                for position in [Led::Top, Led::Bottom, Led::Button] {
                    set_led_mode(chan, position, LedMsg::Reset);
                }
            }
            set_led_color(1, Led::Button, Color::Red);
            set_led_color(2, Led::Button, Color::Red);
        }

        i += 1;
    }

    output_results
}

async fn run_automatic_calibration(
    receiver: &mut I2cFollowerReceiver,
) -> (RegressionValuesInput, RegressionValuesOutput) {
    for i in 0..CHANNELS {
        for &p in LED_POS.iter() {
            reset_led(i, p);
        }
    }

    info!("Waiting for calibration data...");

    let mut current_ch: Option<usize> = None;
    loop {
        match receiver.receive().await {
            I2cFollowerMessage::CalibChannelUpdate(ch) => {
                if Some(ch) != current_ch {
                    if let Some(prev) = current_ch {
                        reset_led(prev, Led::Button);
                    }
                    set_led_color(ch, Led::Button, Color::Yellow);
                    current_ch = Some(ch);
                }
            }
            I2cFollowerMessage::CalibSetRegressionValues(input_values, output_values) => {
                info!("Received calibration data.");
                if let Some(prev) = current_ch {
                    reset_led(prev, Led::Button);
                }
                return (input_values, output_values);
            }
            I2cFollowerMessage::CalibStart => {}
        }
    }
}

// 0-10V steps: 0, 2, 4, 6, 8, 10 V (counts = V * 409.5)
const TEST_STEPS_0_10V: [u16; 6] = [0, 819, 1638, 2457, 3276, 4095];
// ±5V steps: -4, -2, 0, +2, +4 V — capped at ±4V to stay within calibrated range
// count = (V + 5) * 409.5
const TEST_STEPS_NEG5_5V: [u16; 5] = [410, 1229, 2048, 2867, 3686];

async fn run_test_mode(calibration_data: &MaxCalibration) -> ! {
    // Initial state: both passthrough and channels start on 0-10V
    configure_jack(
        0,
        Mode::Mode7(ConfigMode7(
            AVR::InternalRef,
            ADCRANGE::Rg0_10v,
            NSAMPLES::Samples16,
        )),
    )
    .await;
    configure_jack(1, Mode::Mode5(ConfigMode5(DACRANGE::Rg0_10v))).await;
    for ch in 2..CHANNELS {
        configure_jack(ch, Mode::Mode5(ConfigMode5(DACRANGE::Rg0_10v))).await;
    }
    for ch in 0..CHANNELS {
        for &p in LED_POS.iter() {
            reset_led(ch, p);
        }
    }
    set_led_color(0, Led::Button, Color::Cyan);
    set_led_color(1, Led::Button, Color::Cyan);
    for ch in 2..CHANNELS {
        set_led_color(ch, Led::Button, Color::Green);
    }

    info!("Test mode active. Press button 0 to exit.");

    // Single range drives both passthrough and channel outputs
    const STEP_TICKS: usize = 200; // 2 seconds per step
    let mut range: usize = 0;
    let mut step: usize = 0;
    let mut step_ticks: usize = 0;

    // Set initial channel output (step 0, 0-10V)
    let ideal = TEST_STEPS_0_10V[0];
    for ch in 2..CHANNELS {
        let (slope, intercept) = calibration_data.outputs[ch][0];
        let hw = if ideal == 0 {
            0
        } else {
            ((ideal as i64 * slope + intercept + CALIBRATION_SCALE_FACTOR / 2) >> 16).clamp(0, 4095)
                as u16
        };
        MAX_VALUES_DAC[ch].store(hw, Ordering::Relaxed);
    }
    info!(
        "Test [0-10V] step 1/{}: count={}",
        TEST_STEPS_0_10V.len(),
        ideal
    );

    let mut subscriber = EVENT_PUBSUB.subscriber().unwrap();

    loop {
        // --- Passthrough: same range as channel outputs ---
        let (is, ii) = calibration_data.inputs[range];
        let (os1, oi1) = calibration_data.outputs[1][range];
        let raw = MAX_VALUES_ADC[0].load(Ordering::Relaxed);
        let ideal_adc =
            ((raw as i64 * is + ii + CALIBRATION_SCALE_FACTOR / 2) >> 16).clamp(0, 4095) as u16;
        let hw_dac = if range == 0 && ideal_adc == 0 {
            0
        } else {
            ((ideal_adc as i64 * os1 + oi1 + CALIBRATION_SCALE_FACTOR / 2) >> 16).clamp(0, 4095)
                as u16
        };
        MAX_VALUES_DAC[1].store(hw_dac, Ordering::Relaxed);

        // --- Step advance (and range switch when all steps exhausted) ---
        step_ticks += 1;
        if step_ticks >= STEP_TICKS {
            step_ticks = 0;
            let step_count = if range == 0 {
                TEST_STEPS_0_10V.len()
            } else {
                TEST_STEPS_NEG5_5V.len()
            };
            step += 1;
            if step >= step_count {
                step = 0;
                range = 1 - range;
                // Switch passthrough jacks and channel outputs together
                if range == 0 {
                    configure_jack(
                        0,
                        Mode::Mode7(ConfigMode7(
                            AVR::InternalRef,
                            ADCRANGE::Rg0_10v,
                            NSAMPLES::Samples16,
                        )),
                    )
                    .await;
                    configure_jack(1, Mode::Mode5(ConfigMode5(DACRANGE::Rg0_10v))).await;
                    set_led_color(0, Led::Button, Color::Cyan);
                    set_led_color(1, Led::Button, Color::Cyan);
                    for ch in 2..CHANNELS {
                        configure_jack(ch, Mode::Mode5(ConfigMode5(DACRANGE::Rg0_10v))).await;
                        set_led_color(ch, Led::Button, Color::Green);
                    }
                } else {
                    configure_jack(
                        0,
                        Mode::Mode7(ConfigMode7(
                            AVR::InternalRef,
                            ADCRANGE::RgNeg5_5v,
                            NSAMPLES::Samples16,
                        )),
                    )
                    .await;
                    configure_jack(1, Mode::Mode5(ConfigMode5(DACRANGE::RgNeg5_5v))).await;
                    set_led_color(0, Led::Button, Color::Yellow);
                    set_led_color(1, Led::Button, Color::Yellow);
                    for ch in 2..CHANNELS {
                        configure_jack(ch, Mode::Mode5(ConfigMode5(DACRANGE::RgNeg5_5v))).await;
                        set_led_color(ch, Led::Button, Color::Yellow);
                    }
                }
            }
            let ideal = if range == 0 {
                let v = TEST_STEPS_0_10V[step];
                info!(
                    "Test [0-10V] step {}/{}: count={}",
                    step + 1,
                    TEST_STEPS_0_10V.len(),
                    v
                );
                v
            } else {
                let v = TEST_STEPS_NEG5_5V[step];
                info!(
                    "Test [+/-5V] step {}/{}: count={}",
                    step + 1,
                    TEST_STEPS_NEG5_5V.len(),
                    v
                );
                v
            };
            for ch in 2..CHANNELS {
                let (slope, intercept) = calibration_data.outputs[ch][range];
                let hw = if range == 0 && ideal == 0 {
                    0
                } else {
                    ((ideal as i64 * slope + intercept + CALIBRATION_SCALE_FACTOR / 2) >> 16)
                        .clamp(0, 4095) as u16
                };
                MAX_VALUES_DAC[ch].store(hw, Ordering::Relaxed);
            }
        }

        // --- 10ms tick + exit check ---
        let exit_check = async {
            loop {
                if let InputEvent::ButtonDown(0) = subscriber.next_message_pure().await {
                    return;
                }
            }
        };
        match select(Timer::after_millis(10), exit_check).await {
            Either::First(_) => {}
            Either::Second(_) => {
                info!("Exiting test mode.");
                cortex_m::peripheral::SCB::sys_reset();
            }
        }
    }
}

pub async fn run_calibration(mut msg_receiver: I2cFollowerReceiver) {
    CALIBRATING.store(true, Ordering::Relaxed);

    set_led_color(0, Led::Button, Color::Yellow);

    info!("Press button or send i2c signal to start calibration");

    let calibration_data = match select(
        wait_for_button_press(0),
        wait_for_start_cmd(&mut msg_receiver),
    )
    .await
    {
        Either::First(_) => {
            // Manual calibration
            info!("Starting manual calibration...");
            let inputs = run_manual_input_calibration().await;
            let outputs = run_manual_output_calibration().await;

            MaxCalibration { inputs, outputs }
        }
        Either::Second(_) => {
            // Automatic calibration
            info!("Starting automatic calibration...");
            let (inputs, outputs) = run_automatic_calibration(&mut msg_receiver).await;

            MaxCalibration { inputs, outputs }
        }
    };

    store_calibration_data(&calibration_data).await;

    for chan in 0..16 {
        for &p in LED_POS.iter() {
            flash_led(chan, p, Color::Green, Some(5));
        }
    }

    info!("Calibration done. Entering test mode...");

    Timer::after_secs(2).await;
    run_test_mode(&calibration_data).await;
}
