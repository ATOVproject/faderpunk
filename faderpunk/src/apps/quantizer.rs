use config::Config;
use libfp::quantizer::{Key, Note};

use crate::app::{App, Led, Range};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 0;

pub static CONFIG: Config<PARAMS> = Config::new("Quantizer", "Quantize to clock");

pub async fn run(app: App<CHANNELS>) {
    let in_jack = app.make_in_jack(0, Range::_0_10V).await;
    let out_jack = app.make_out_jack(1, Range::_0_10V).await;

    let mut quantizer = app.use_quantizer();
    quantizer.set_scale(Key::PentatonicMajor, Note::C, Note::C);
    let mut clock = app.use_clock();
    app.set_led(0, Led::Button, (50, 50, 50), 0);

    loop {
        clock.wait_for_tick(1).await;
        let value = in_jack.get_value();
        let out = ((quantizer.get_quantized_voltage(value) / 10.0) * 4095.0) as u16;
        out_jack.set_value(out);
        app.set_led(0, Led::Button, (50, 50, 50), 150);
        app.delay_millis(200).await;
        app.set_led(0, Led::Button, (50, 50, 50), 0);
    }
}
