use config::Value;

use crate::storage::{Store, APP_STORAGE_CMD_PUBSUB};

register_apps!(
    1 => default,
    2 => lfo,
    3 => clock_test,
    4 => ad,
    5 => seq8,
    6 => automator,
    7 => clkcvrnd,
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
