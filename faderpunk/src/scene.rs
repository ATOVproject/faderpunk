use portable_atomic::{AtomicU8, Ordering};

static SCENE: AtomicU8 = AtomicU8::new(0);

pub fn get_scene() -> u8 {
    SCENE.load(Ordering::Relaxed)
}

pub fn set_scene(scene: u8) {
    SCENE.store(scene, Ordering::Relaxed);
}
