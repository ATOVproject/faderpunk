use crate::app::App;
use defmt::info;

register_apps!(
    1 => default,
    2 => measure,
    3 => lfo,
    4 => ad,
    5 => freezer,
    6 => ping,
    7 => default_tester
    
);
