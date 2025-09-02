use embassy_futures::select::select;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use libfp::{
    utils::{is_close, split_unsigned_value},
    Brightness, Color,
};

use libfp::{Config, Range};

use crate::app::{App, Led};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 0;

pub static CONFIG: Config<PARAMS> = Config::new("Cal Unipolar", "Unipolar range calibration test");

const LED_COLOR: Color = Color::Violet;
const BUTTON_BRIGHTNESS: Brightness = Brightness::Lower;

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let app_loop = async {
        loop {
            run(&app).await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

// Voltage to DAC value mapping for unipolar range (0V to +10V mapped to 0-4095)
// 0V = 0, 1V = 409.5, 2V = 819, etc.
const VOLTAGE_TARGETS: [(i32, u16); 11] = [
    (0, 0),     // 0V
    (1, 410),   // 1V
    (2, 819),   // 2V
    (3, 1229),  // 3V
    (4, 1638),  // 4V
    (5, 2048),  // 5V
    (6, 2457),  // 6V
    (7, 2867),  // 7V
    (8, 3276),  // 8V
    (9, 3686),  // 9V
    (10, 4095), // 10V
];

const SNAP_THRESHOLD: u16 = 50; // How close fader needs to be to snap

fn get_voltage_color(voltage: i32) -> Color {
    match voltage {
        0 => Color::Red,
        1 => Color::Orange,
        2 => Color::Yellow,
        3 => Color::Green,
        4 => Color::Cyan,
        5 => Color::Blue,
        6 => Color::Pink,
        7 => Color::White,
        8 => Color::Sand,
        9 => Color::SkyBlue,
        10 => Color::PaleGreen,
        _ => LED_COLOR,
    }
}

fn find_snap_target(fader_value: u16) -> Option<(i32, u16)> {
    for &(voltage, target) in &VOLTAGE_TARGETS {
        if is_close(fader_value, target)
            || (fader_value as i32 - target as i32).abs() < SNAP_THRESHOLD as i32
        {
            return Some((voltage, target));
        }
    }
    None
}

pub async fn run(app: &App<CHANNELS>) {
    let fader = app.use_faders();
    let leds = app.use_leds();

    // Set up unipolar output jack (0V to +10V range)
    let jack = app.make_out_jack(0, Range::_0_10V).await;

    let mut output_value = 0u16; // Start at 0V
    let mut snapped_voltage: Option<i32> = None;

    // Set initial output
    jack.set_value(output_value);

    loop {
        app.delay_millis(1).await;

        let fader_value = fader.get_value();

        // Check if we should snap to a target voltage
        if let Some((voltage, target_dac)) = find_snap_target(fader_value) {
            if snapped_voltage != Some(voltage) {
                snapped_voltage = Some(voltage);
                output_value = target_dac;

                // Update LEDs to show snapped voltage
                let color = get_voltage_color(voltage);
                leds.set(0, Led::Button, color, BUTTON_BRIGHTNESS);

                let led_parts = split_unsigned_value(output_value);
                leds.set(0, Led::Top, color, Brightness::Custom(led_parts[0]));
                leds.set(0, Led::Bottom, color, Brightness::Custom(led_parts[1]));
            }
        }
        // If no snap target, keep the previous output_value unchanged

        jack.set_value(output_value);
    }
}
