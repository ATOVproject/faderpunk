use embassy_futures::select::{select, select3};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    AppIcon, Brightness, Color, CombineMode, Config, Param, Range, Value, APP_MAX_PARAMS,
};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 2;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Combiner / Mixer (TODO)",
    "Combine and mix internal routes onto output",
    Color::Cyan,
    AppIcon::Attenuate,
)
.add_param(Param::CombineMode {
    name: "Mode",
    variants: &[
        CombineMode::Sum,
        CombineMode::Average,
        CombineMode::Max,
        CombineMode::Min,
        CombineMode::Or,
        CombineMode::And,
        CombineMode::Xor,
    ],
})
.add_param(Param::Range {
    name: "Range",
    variants: &[Range::_0_10V, Range::_Neg5_5V],
});

#[derive(Clone, Copy)]
pub struct Params {
    mode: CombineMode,
    range: Range,
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            mode: CombineMode::from_value(values[0]),
            range: Range::from_value(values[1]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.mode.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct Storage {
    master_gain: u16,
}

impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            mode: CombineMode::Sum,
            range: Range::_0_10V,
        },
    );
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
    let faders = app.use_faders();
    let leds = app.use_leds();

    let range = params.query(|p| p.range);

    let out_jack = app.make_out_jack(0, range).await;
    let in_jack = app.make_in_jack(0, range).await;

    leds.set(0, Led::Bottom, Color::Cyan, Brightness::High);

    loop {
        faders.wait_for_any_change().await;

        let gain = faders.get_value_at(0);
        storage.modify(|s| s.master_gain = gain);

        let master_gain = storage.query(|s| s.master_gain);
        let input_val = in_jack.get_value();
        let output_val = ((input_val as u32 * master_gain as u32) / 4095) as u16;
        out_jack.set_value(output_val);
    }
}
