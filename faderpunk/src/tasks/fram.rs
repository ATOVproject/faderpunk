//! FM24V10 FRAM backend for fp-core's storage service.

use core::future::pending;
use embassy_executor::Spawner;
use embassy_rp::{
    i2c::{Async, I2c},
    peripherals::I2C1,
};
use fm24v10::Fm24v10;
use libfp::Color;

use fp_core::app::Led;
use fp_core::tasks::fram::{run_storage_service, StorageBackend};
use fp_core::tasks::leds::{set_led_overlay_mode, LedMode};

type Fram = Fm24v10<'static, I2c<'static, I2C1, Async>>;

struct FramBackend(Fram);

impl StorageBackend for FramBackend {
    async fn read(&mut self, address: u32, buf: &mut [u8]) -> Result<(), ()> {
        self.0.read(address, buf).await.map_err(|_| ())
    }

    async fn write(&mut self, address: u32, data: &[u8]) -> Result<(), ()> {
        self.0.write(address, data).await.map_err(|_| ())
    }
}

pub async fn start_fram(spawner: &Spawner, mut fram: Fram) {
    // Initialization check. If fram can't be read from, stall and flash LED
    let mut len_bytes: [u8; 2] = [0; 2];
    if fram.read(0, &mut len_bytes).await.is_err() {
        set_led_overlay_mode(0, Led::Button, LedMode::Flash(Color::Red, None)).await;
        pending::<()>().await;
    }

    spawner.spawn(run_fram(fram)).unwrap();
}

#[embassy_executor::task]
async fn run_fram(fram: Fram) {
    run_storage_service(FramBackend(fram)).await
}
