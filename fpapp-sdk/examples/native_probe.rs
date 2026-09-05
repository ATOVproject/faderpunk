#![no_std]
#![no_main]

use core::future::Future;
use fpapp_sdk::{EventReader, HostV1};

fn app_future(host: *const HostV1) -> impl Future<Output = ()> {
    async move {
        let mut total = 0u16;
        let mut events = unsafe { EventReader::new(host) };
        loop {
            let event = events.next_event().await;
            total = total.wrapping_add(event.value);
            unsafe { ((*host).set_output)((*host).context, event.channel, total) };
        }
    }
}

fpapp_sdk::export_app!(app_future);

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {}
}
