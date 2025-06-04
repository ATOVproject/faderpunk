use config::Value;

use crate::storage::{Store, APP_STORAGE_CMD_PUBSUB};

register_apps!(
    1 => default,
    2 => lfo,
    3 => measure,
    4 => trigger,
    5 => clock_test,
);

pub async fn temp_param_loop() {
    let temp_params = Store::new([Value::bool(true), Value::i32(4000), Value::f32(4.5)], 1, 1);
    let mut subscriber = APP_STORAGE_CMD_PUBSUB.subscriber().unwrap();
    loop {
        match subscriber.next_message_pure().await {
            _ => {}
        }
    }
}
