mod resources;
mod utils;
mod pattern_generator;

// Re-export public module members
pub use pattern_generator::{
    Options,
    OutputMode,
    PatternGenerator,
    PatternGeneratorSettings,
};

pub use resources::{
    K_NUM_PARTS,
    K_NUM_STEPS_PER_PATTERN
};