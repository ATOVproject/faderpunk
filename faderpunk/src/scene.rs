// use portable_atomic::{AtomicU8, Ordering};
//
// // TODO: store last scene on eeprom
// pub static CURRENT_SCENE: AtomicU8 = AtomicU8::new(0);
//
// pub fn get_global_scene() -> u8 {
//     CURRENT_SCENE.load(Ordering::Relaxed)
// }
//
// pub fn set_global_scene(scene: u8) {
//     CURRENT_SCENE.store(scene.clamp(0, 15), Ordering::Relaxed);
// }
