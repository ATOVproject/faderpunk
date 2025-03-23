use crate::app::App;
use defmt::info;

register_apps!(
    1 => default,
    2 => lfo,
    3 => ad,
    4 => cv2midi, //Add LEDs
    5 => trigger,
    6 => seq8

);
