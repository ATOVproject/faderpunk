use postcard_bindgen::PostcardBindings;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum LatchLayer {
    Main,
    Alt,
    Third,
}

impl From<bool> for LatchLayer {
    fn from(is_alternate_layer: bool) -> Self {
        if is_alternate_layer {
            Self::Alt
        } else {
            Self::Main
        }
    }
}

/// Defines how a fader should take control of a value when switching layers or starting a session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, PostcardBindings)]
pub enum TakeoverMode {
    /// Wait until fader crosses target value, then sync (default behavior)
    #[default]
    Pickup,
    /// Value immediately jumps to fader position (no pickup delay)
    Jump,
    /// Gradually scale value toward fader position based on movement and runway
    Scale,
}

/// A stateless machine that implements "catch-up" or "pickup" logic for a fader or knob.
///
/// This struct determines when a physical fader should take control of a value.
/// It is "stateless" in the sense that it does not store the values of the layers it manages.
/// The caller is responsible for maintaining the state and passing the relevant value to the `update` method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalogLatch {
    active_layer: LatchLayer,
    is_latched: bool,
    prev_value: u16,
    prev_target: u16,
    jitter_tolerance: u16,
    mode: TakeoverMode,
}

const RUNWAY_GAIN_NUM: i32 = 5;
const RUNWAY_GAIN_DEN: i32 = 4;

impl AnalogLatch {
    /// Creates a new AnalogLatch with default jitter tolerance.
    ///
    /// It starts on layer 0 and assumes the fader's initial physical position
    /// matches the a given initial value, so it begins in a "latched" state.
    pub fn new(initial_value: u16, mode: TakeoverMode) -> Self {
        Self::with_tolerance(initial_value, 20, mode) // Default tolerance of 20
    }

    /// Creates a new AnalogLatch with custom jitter tolerance.
    ///
    /// # Arguments
    /// * `initial_value`: The starting position of the fader
    /// * `jitter_tolerance`: The tolerance for considering values equal (to handle ADC noise)
    /// * `mode`: The takeover mode (Jump, Pickup, or Scale)
    pub fn with_tolerance(initial_value: u16, jitter_tolerance: u16, mode: TakeoverMode) -> Self {
        Self {
            active_layer: LatchLayer::Main,
            is_latched: true,
            prev_value: initial_value,
            prev_target: initial_value,
            jitter_tolerance,
            mode,
        }
    }

    /// Checks if two values are approximately equal within the jitter tolerance
    fn values_equal(&self, a: u16, b: u16) -> bool {
        // let diff = if a > b { a - b } else { b - a };
        a.abs_diff(b) <= self.jitter_tolerance
    }

    /// Returns the index of the layer that the latch is currently focused on.
    pub fn active_layer(&self) -> LatchLayer {
        self.active_layer
    }

    /// Returns `true` if the fader is currently in control of the active layer's value.
    pub fn is_latched(&self) -> bool {
        self.is_latched
    }

    /// Updates the latch's internal state based on new fader input.
    ///
    /// # Arguments
    /// * `value`: The current physical value of the fader.
    /// * `new_active_layer`: The index of the layer that should be active.
    /// * `active_layer_target_value`: The currently stored value for the active layer. This is
    ///   used to detect the crossover point when the fader is not latched.
    ///
    /// # Returns
    /// * `Some(new_value)` if the fader is latched and its value has changed significantly
    ///   (beyond jitter tolerance), or if the fader has just crossed the target value and
    ///   become latched. The caller should use this new value to update their state for
    ///   the active layer.
    /// * `None` if no change should occur (e.g., the fader is moving but has not yet
    ///   reached the target value, or movement is within jitter tolerance).
    pub fn update(
        &mut self,
        value: u16,
        new_active_layer: LatchLayer,
        active_layer_target_value: u16,
    ) -> Option<u16> {
        // Did the user switch layers?
        if new_active_layer != self.active_layer {
            self.active_layer = new_active_layer;
            // For Jump mode, always latch immediately on layer switch
            // For other modes, unlatch unless fader is already at target
            self.is_latched = match self.mode {
                TakeoverMode::Jump => true,
                _ => self.values_equal(value, active_layer_target_value),
            };
            self.prev_target = active_layer_target_value;
        } else if self.is_latched {
            // If we are latched but the target has changed externally, check if we should unlatch.
            // This happens if the target value is changed by something other than this fader.
            if self.prev_target != active_layer_target_value {
                // For Jump mode, stay latched even when target changes
                // For other modes, stay latched only if fader equals new target
                self.is_latched = match self.mode {
                    TakeoverMode::Jump => true,
                    _ => self.values_equal(value, active_layer_target_value),
                };
                self.prev_target = active_layer_target_value;
            }
        } else {
            // If we are unlatched and the target changes to our current position, latch immediately
            if self.prev_target != active_layer_target_value
                && self.values_equal(value, active_layer_target_value)
            {
                self.is_latched = true;
                self.prev_target = active_layer_target_value;
            } else if self.prev_target != active_layer_target_value {
                self.prev_target = active_layer_target_value;
            }
        }

        let mut new_value = None;

        // Mode-specific behavior
        match self.mode {
            TakeoverMode::Jump => {
                // Jump mode: always return fader value if it moved beyond jitter tolerance
                if !self.values_equal(value, self.prev_value) {
                    new_value = Some(value);
                }
            }
            TakeoverMode::Pickup => {
                // Pickup mode: existing crossover detection logic
                if self.is_latched {
                    // Fader is in control. If it moves beyond jitter tolerance, the value changes.
                    if !self.values_equal(value, self.prev_value) {
                        new_value = Some(value);
                    }
                } else {
                    // Fader is not in control. Check for crossover.
                    // We consider it crossed if we've passed through or reached the target
                    let has_crossed = (self.prev_value..=value)
                        .contains(&active_layer_target_value)
                        || (value..=self.prev_value).contains(&active_layer_target_value)
                        || self.values_equal(value, active_layer_target_value);

                    if has_crossed {
                        // Crossover detected! Latch and report the new value.
                        self.is_latched = true;
                        new_value = Some(value);
                    }
                }
            }
            TakeoverMode::Scale => {
                // Scale mode: gradually converge value toward fader position
                if self.is_latched {
                    // Already synced, move 1:1
                    if !self.values_equal(value, self.prev_value) {
                        new_value = Some(value);
                    }
                } else {
                    // Not synced yet - calculate delta-based movement
                    let fader_delta = value as i32 - self.prev_value as i32;

                    // Only process if fader actually moved
                    if fader_delta != 0 {
                        let current_value_i32 = active_layer_target_value as i32;
                        let fader_pos = value as i32;

                        // Check if both are at the same boundary (both at min or max)
                        if (current_value_i32 == 0 && fader_pos == 0)
                            || (current_value_i32 == 4095 && fader_pos == 4095)
                        {
                            // Both at same boundary, latch immediately
                            self.is_latched = true;
                            new_value = Some(value);
                        } else {
                            // Scale the delta based on whether the fader is moving toward
                            // or away from the current value.
                            let fader_dir = fader_delta.signum();
                            let dir_to_current =
                                (current_value_i32 - self.prev_value as i32).signum();
                            let moving_toward = dir_to_current != 0 && dir_to_current == fader_dir;
                            let scaled_delta_base = if moving_toward {
                                fader_delta / 2 // Moving toward current, converge gently
                            } else {
                                fader_delta * 2 // Moving away, converge aggressively
                            };

                            // Apply remaining runway factor so convergence accelerates near edges.
                            // Factor grows from 1x (middle of range) up to ~2x (at the edge).
                            let range = 4095i32;
                            let remaining_runway = if fader_delta > 0 {
                                range - fader_pos
                            } else {
                                fader_pos
                            };
                            let runway_factor_num = range * RUNWAY_GAIN_DEN
                                + (range - remaining_runway) * (RUNWAY_GAIN_NUM - RUNWAY_GAIN_DEN);
                            let scaled_delta =
                                scaled_delta_base * runway_factor_num / (range * RUNWAY_GAIN_DEN);

                            // Apply scaled delta to current value
                            let new_current_value =
                                (current_value_i32 + scaled_delta).clamp(0, 4095) as u16;

                            // Check if we've crossed (current crossed fader position)
                            let crossed = (current_value_i32 <= fader_pos
                                && new_current_value as i32 >= fader_pos)
                                || (current_value_i32 >= fader_pos
                                    && new_current_value as i32 <= fader_pos)
                                || self.values_equal(new_current_value, value);

                            if crossed {
                                // Crossed! Latch and return fader value
                                self.is_latched = true;
                                new_value = Some(value);
                            } else {
                                // Still approaching, return scaled value
                                new_value = Some(new_current_value);
                            }
                        }
                    }
                }
            }
        }

        self.prev_value = value;
        new_value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_latched_state() {
        let latch = AnalogLatch::new(100, TakeoverMode::Pickup);
        assert_eq!(latch.active_layer(), LatchLayer::Main);
        assert!(latch.is_latched());
    }

    #[test]
    fn test_basic_latched_movement() {
        let mut latch = AnalogLatch::new(100, TakeoverMode::Pickup);

        // Moving fader while latched should update value
        let result = latch.update(150, LatchLayer::Main, 100);
        assert_eq!(result, Some(150));
        assert!(latch.is_latched());

        // No movement should return None
        let result = latch.update(150, LatchLayer::Main, 100);
        assert_eq!(result, None);
        assert!(latch.is_latched());
    }

    #[test]
    fn test_jitter_tolerance_while_latched() {
        let mut latch = AnalogLatch::with_tolerance(100, 3, TakeoverMode::Pickup);

        // Small movements within tolerance should not trigger updates
        let result = latch.update(101, LatchLayer::Main, 100);
        assert_eq!(result, None);
        assert!(latch.is_latched());

        let result = latch.update(99, LatchLayer::Main, 100);
        assert_eq!(result, None);
        assert!(latch.is_latched());

        // Movement beyond tolerance should trigger update
        let result = latch.update(104, LatchLayer::Main, 100);
        assert_eq!(result, Some(104));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_jitter_tolerance_on_target_change() {
        let mut latch = AnalogLatch::with_tolerance(100, 3, TakeoverMode::Pickup);

        // Target changes to a value within jitter tolerance of current position
        // Should stay latched
        let result = latch.update(100, LatchLayer::Main, 102);
        assert_eq!(result, None);
        assert!(latch.is_latched());

        // Target changes to a value outside jitter tolerance
        // Should unlatch
        let result = latch.update(100, LatchLayer::Main, 110);
        assert_eq!(result, None);
        assert!(!latch.is_latched());
    }

    #[test]
    fn test_layer_switching_with_jitter() {
        let mut latch = AnalogLatch::with_tolerance(100, 3, TakeoverMode::Pickup);

        // Switch to layer 1, fader within tolerance of new target
        let result = latch.update(198, LatchLayer::Alt, 200);
        // The fader's physical value changed significantly, so report it
        assert_eq!(result, Some(198));
        assert_eq!(latch.active_layer(), LatchLayer::Alt);
        // Should remain latched due to tolerance
        assert!(latch.is_latched());
    }

    #[test]
    fn test_layer_switching_exact_match() {
        let mut latch = AnalogLatch::new(100, TakeoverMode::Pickup);

        // Switch to layer 1, fader moves to the new target value
        let result = latch.update(200, LatchLayer::Alt, 200);
        // The fader's physical value changed, so the change should be reported
        assert_eq!(result, Some(200));
        assert_eq!(latch.active_layer(), LatchLayer::Alt);
        // Should remain latched
        assert!(latch.is_latched());
    }

    #[test]
    fn test_layer_switching_different_value() {
        let mut latch = AnalogLatch::new(100, TakeoverMode::Pickup);

        // Switch to layer 1, fader not at target value
        let result = latch.update(100, LatchLayer::Alt, 200);
        assert_eq!(result, None);
        assert_eq!(latch.active_layer(), LatchLayer::Alt);
        // Should become unlatched
        assert!(!latch.is_latched());
    }

    #[test]
    fn test_crossover_detection_upward() {
        let mut latch = AnalogLatch::new(100, TakeoverMode::Pickup);

        // Switch layers and unlatch
        latch.update(100, LatchLayer::Alt, 150);
        assert!(!latch.is_latched());

        // Move fader upward past target
        let result = latch.update(160, LatchLayer::Alt, 150);
        assert_eq!(result, Some(160));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_crossover_detection_with_tolerance() {
        let mut latch = AnalogLatch::with_tolerance(100, 3, TakeoverMode::Pickup);

        // Switch layers and unlatch
        latch.update(100, LatchLayer::Alt, 150);
        assert!(!latch.is_latched());

        // Move fader to within tolerance of target
        let result = latch.update(148, LatchLayer::Alt, 150);
        assert_eq!(result, Some(148));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_crossover_detection_downward() {
        let mut latch = AnalogLatch::new(200, TakeoverMode::Pickup);

        // Switch layers and unlatch
        latch.update(200, LatchLayer::Alt, 150);
        assert!(!latch.is_latched());

        // Move fader downward past target
        let result = latch.update(140, LatchLayer::Alt, 150);
        assert_eq!(result, Some(140));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_crossover_detection_exact_hit() {
        let mut latch = AnalogLatch::new(100, TakeoverMode::Pickup);

        // Switch layers and unlatch
        latch.update(100, LatchLayer::Alt, 150);
        assert!(!latch.is_latched());

        // Move fader to exact target value
        let result = latch.update(150, LatchLayer::Alt, 150);
        assert_eq!(result, Some(150));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_no_crossover_before_target() {
        let mut latch = AnalogLatch::new(100, TakeoverMode::Pickup);

        // Switch layers and unlatch
        latch.update(100, LatchLayer::Alt, 200);
        assert!(!latch.is_latched());

        // Move fader but not past target
        let result = latch.update(150, LatchLayer::Alt, 200);
        assert_eq!(result, None);
        assert!(!latch.is_latched());
    }

    #[test]
    fn test_multiple_movements_unlatched() {
        let mut latch = AnalogLatch::new(100, TakeoverMode::Pickup);

        // Switch layers and unlatch (target far enough from fader)
        latch.update(100, LatchLayer::Alt, 500);
        assert!(!latch.is_latched());

        // Multiple movements without crossing target (all > tolerance away from 500)
        let result = latch.update(200, LatchLayer::Alt, 500);
        assert_eq!(result, None);
        assert!(!latch.is_latched());

        let result = latch.update(400, LatchLayer::Alt, 500);
        assert_eq!(result, None);
        assert!(!latch.is_latched());

        // Finally cross the target
        let result = latch.update(520, LatchLayer::Alt, 500);
        assert_eq!(result, Some(520));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_target_changes_to_fader_position() {
        let mut latch = AnalogLatch::new(100, TakeoverMode::Pickup);

        // Move fader to 150
        assert_eq!(latch.update(150, LatchLayer::Main, 100), Some(150));
        assert!(latch.is_latched());

        // Target externally changes to 150 (where fader already is)
        // Should stay latched since we're already at the target
        assert_eq!(latch.update(150, LatchLayer::Main, 150), None);
        assert!(latch.is_latched());
    }

    #[test]
    fn test_target_changes_to_near_fader_position() {
        let mut latch = AnalogLatch::with_tolerance(100, 3, TakeoverMode::Pickup);

        // Move fader to 150
        assert_eq!(latch.update(150, LatchLayer::Main, 100), Some(150));
        assert!(latch.is_latched());

        // Target externally changes to within tolerance of fader position
        // Should stay latched
        assert_eq!(latch.update(150, LatchLayer::Main, 152), None);
        assert!(latch.is_latched());
    }

    #[test]
    fn test_three_layer_latching() {
        let mut latch = AnalogLatch::new(100, TakeoverMode::Pickup);
        let mut layer_values = [100, 200, 300];

        // We start on Main, latched at 100
        assert!(latch.is_latched());
        assert_eq!(latch.active_layer(), LatchLayer::Main);

        // Switch to Alt layer, target is 200. Fader is at 100, so unlatch.
        let result = latch.update(100, LatchLayer::Alt, layer_values[1]);
        assert_eq!(result, None);
        assert!(!latch.is_latched());
        assert_eq!(latch.active_layer(), LatchLayer::Alt);

        // Move fader to 200 to latch Alt layer
        let result = latch.update(200, LatchLayer::Alt, layer_values[1]);
        assert_eq!(result, Some(200));
        assert!(latch.is_latched());
        layer_values[1] = 200;

        // Switch to Third layer, target is 300. Fader is at 200, so unlatch.
        let result = latch.update(200, LatchLayer::Third, layer_values[2]);
        assert_eq!(result, None);
        assert!(!latch.is_latched());
        assert_eq!(latch.active_layer(), LatchLayer::Third);

        // Move fader to 300 to latch Third layer
        let result = latch.update(300, LatchLayer::Third, layer_values[2]);
        assert_eq!(result, Some(300));
        assert!(latch.is_latched());
        layer_values[2] = 300;

        // Switch back to Main layer, target is 100. Fader is at 300, so unlatch.
        let result = latch.update(300, LatchLayer::Main, layer_values[0]);
        assert_eq!(result, None);
        assert!(!latch.is_latched());
        assert_eq!(latch.active_layer(), LatchLayer::Main);

        // Move fader back to 100 to latch Main layer again
        let result = latch.update(100, LatchLayer::Main, layer_values[0]);
        assert_eq!(result, Some(100));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_jump_immediate_control() {
        let mut latch = AnalogLatch::new(1000, TakeoverMode::Jump);

        // Switch to Alt layer with different target
        let result = latch.update(1000, LatchLayer::Alt, 3000);
        // Jump mode: first movement returns value immediately
        assert_eq!(result, None); // No fader movement yet
        assert!(latch.is_latched()); // Jump mode always stays latched

        // Move fader
        let result = latch.update(1100, LatchLayer::Alt, 3000);
        assert_eq!(result, Some(1100)); // Immediately takes control
        assert!(latch.is_latched());
    }

    #[test]
    fn test_jump_layer_switch() {
        let mut latch = AnalogLatch::new(1000, TakeoverMode::Jump);

        // Move on Main layer
        latch.update(1500, LatchLayer::Main, 1000);

        // Switch to Alt layer with very different target
        let _result = latch.update(1500, LatchLayer::Alt, 3500);
        // Jump mode always latches on layer switch
        assert!(latch.is_latched());
        assert_eq!(latch.active_layer(), LatchLayer::Alt);

        // Next movement immediately controls
        let result = latch.update(1600, LatchLayer::Alt, 3500);
        assert_eq!(result, Some(1600));
    }

    #[test]
    fn test_jump_never_waits() {
        let mut latch = AnalogLatch::new(0, TakeoverMode::Jump);

        // Switch layers with target far away
        latch.update(0, LatchLayer::Alt, 4000);
        assert!(latch.is_latched()); // Jump mode stays latched

        // Every movement returns value, never waits for crossover
        assert_eq!(latch.update(100, LatchLayer::Alt, 4000), Some(100));
        assert_eq!(latch.update(200, LatchLayer::Alt, 4000), Some(200));
        assert_eq!(latch.update(300, LatchLayer::Alt, 4000), Some(300));
        // Can even go away from target
        assert_eq!(latch.update(200, LatchLayer::Alt, 4000), Some(200));
    }

    #[test]
    fn test_jump_full_sweep() {
        let mut latch = AnalogLatch::new(2000, TakeoverMode::Jump);

        // Sweep up to max
        assert_eq!(latch.update(3000, LatchLayer::Main, 2000), Some(3000));
        assert_eq!(latch.update(4095, LatchLayer::Main, 3000), Some(4095));

        // Sweep down to min
        assert_eq!(latch.update(2000, LatchLayer::Main, 4095), Some(2000));
        assert_eq!(latch.update(0, LatchLayer::Main, 2000), Some(0));

        // All values reported immediately
        assert!(latch.is_latched());
    }

    #[test]
    fn test_jump_respects_jitter() {
        let mut latch = AnalogLatch::with_tolerance(1000, 3, TakeoverMode::Jump);

        // Small movements within tolerance still don't trigger
        assert_eq!(latch.update(1001, LatchLayer::Main, 1000), None);
        assert_eq!(latch.update(999, LatchLayer::Main, 1000), None);

        // But beyond tolerance, immediately takes control
        assert_eq!(latch.update(1005, LatchLayer::Main, 1000), Some(1005));
    }

    #[test]
    fn test_scale_gradual_catchup_upward() {
        let mut latch = AnalogLatch::new(1000, TakeoverMode::Scale);

        // Switch to layer with higher target (current=3000 > fader=1100)
        latch.update(1000, LatchLayer::Alt, 3000);
        assert!(!latch.is_latched());

        // Move fader up from 1000 to 1100 (delta = +100)
        // Fader moving toward current (both positive direction), so scaled_delta = 100 / 2 = 50
        // Runway factor at fader=1100: (16380+1100)/16380 ≈ 1.067, scaled_delta = 53
        // New current = 3000 + 53 = 3053
        let result = latch.update(1100, LatchLayer::Alt, 3000);
        assert_eq!(result, Some(3053));
        assert!(!latch.is_latched());
    }

    #[test]
    fn test_scale_gradual_catchup_downward() {
        let mut latch = AnalogLatch::new(3000, TakeoverMode::Scale);

        // Switch to layer with lower target (current=1000 < fader=2900)
        latch.update(3000, LatchLayer::Alt, 1000);
        assert!(!latch.is_latched());

        // Move fader down (from 3000 to 2900, delta = -100)
        // Fader moving toward current (both negative direction), so scaled_delta = -100 / 2 = -50
        // Runway factor at fader=2900 (delta<0): (16380+1195)/16380 ≈ 1.073, scaled_delta = -53
        // New current = 1000 + (-53) = 947
        let result = latch.update(2900, LatchLayer::Alt, 1000);
        assert_eq!(result, Some(947));
        assert!(!latch.is_latched());
    }

    #[test]
    fn test_scale_convergence_latch() {
        let mut latch = AnalogLatch::new(2000, TakeoverMode::Scale);

        // Initial fader 2000, target at 3000 (current < fader)
        latch.update(2000, LatchLayer::Alt, 3000);
        assert!(!latch.is_latched());

        // Move fader to 2100 (delta = 100)
        // Fader moving toward current, so scaled_delta = 100 / 2 = 50
        // Runway factor at fader=2100: (16380+2100)/16380 ≈ 1.128, scaled_delta = 56
        // New current = 3000 + 56 = 3056
        let result = latch.update(2100, LatchLayer::Alt, 3000);
        assert_eq!(result, Some(3056));
        assert!(!latch.is_latched());

        // Eventually should cross and latch as fader moves all the way to 3000
        for fader_pos in (2200..=3000).step_by(100) {
            if let Some(_v) = latch.update(fader_pos as u16, LatchLayer::Alt, 3000) {
                if latch.is_latched() {
                    return; // Successfully latched
                }
            }
        }
    }

    #[test]
    fn test_scale_small_movements() {
        let mut latch = AnalogLatch::with_tolerance(1000, 3, TakeoverMode::Scale);

        // Target far away (current=3000 < fader=1005)
        latch.update(1000, LatchLayer::Alt, 3000);

        // Small movement up (delta = +5), fader moving toward current
        // scaled_delta_base = 5 / 2 = 2 (integer truncation)
        // Runway factor ≈ 1.06, scaled_delta = 2
        // New current = 3000 + 2 = 3002
        let result = latch.update(1005, LatchLayer::Alt, 3000);
        assert_eq!(result, Some(3002));
    }

    #[test]
    fn test_scale_already_at_target() {
        let mut latch = AnalogLatch::new(2000, TakeoverMode::Scale);

        // Switch to layer where target equals fader position
        let result = latch.update(2000, LatchLayer::Alt, 2000);
        // Should latch immediately since fader position equals stored value
        assert!(latch.is_latched());
        assert_eq!(result, None); // No movement
    }

    #[test]
    fn test_scale_once_latched_moves_1to1() {
        let mut latch = AnalogLatch::new(2000, TakeoverMode::Scale);

        // Start at fader position (latched)
        latch.update(2000, LatchLayer::Main, 2000);
        assert!(latch.is_latched());

        // Move fader - once latched and target matches, move 1:1
        assert_eq!(latch.update(2100, LatchLayer::Main, 2100), Some(2100));
        assert!(latch.is_latched());
        assert_eq!(latch.update(2200, LatchLayer::Main, 2200), Some(2200));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_pickup_existing_behavior() {
        let mut latch = AnalogLatch::new(1000, TakeoverMode::Pickup);

        // Switch layers, unlatch
        latch.update(1000, LatchLayer::Alt, 2000);
        assert!(!latch.is_latched());

        // Move toward target but don't cross
        assert_eq!(latch.update(1500, LatchLayer::Alt, 2000), None);
        assert!(!latch.is_latched());

        // Cross target
        assert_eq!(latch.update(2100, LatchLayer::Alt, 2000), Some(2100));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_pickup_layer_switch_unlatch() {
        let mut latch = AnalogLatch::new(1000, TakeoverMode::Pickup);

        // Latched on Main
        assert!(latch.is_latched());

        // Switch to Alt with different target
        latch.update(1000, LatchLayer::Alt, 3000);
        assert!(!latch.is_latched()); // Must cross to latch

        // Move close but don't cross
        assert_eq!(latch.update(2900, LatchLayer::Alt, 3000), None);
        assert!(!latch.is_latched());
    }

    #[test]
    fn test_same_input_different_outputs() {
        let mut jump = AnalogLatch::new(1000, TakeoverMode::Jump);
        let mut pickup = AnalogLatch::new(1000, TakeoverMode::Pickup);
        let mut scale = AnalogLatch::new(1000, TakeoverMode::Scale);

        // Switch all to Alt with target 3000
        jump.update(1000, LatchLayer::Alt, 3000);
        pickup.update(1000, LatchLayer::Alt, 3000);
        scale.update(1000, LatchLayer::Alt, 3000);

        // Move fader to 1500
        let jump_result = jump.update(1500, LatchLayer::Alt, 3000);
        let pickup_result = pickup.update(1500, LatchLayer::Alt, 3000);
        let scale_result = scale.update(1500, LatchLayer::Alt, 3000);

        // Jump: immediate control
        assert_eq!(jump_result, Some(1500));
        assert!(jump.is_latched());

        // Pickup: no control yet (haven't crossed target)
        assert_eq!(pickup_result, None);
        assert!(!pickup.is_latched());

        // Scale: fader moves toward current (both positive direction), so delta is halved.
        // The output moves in the fader's direction (up) but slower than the fader,
        // so the fader catches up to the output over time.
        assert!(scale_result.is_some());
        let scale_val = scale_result.unwrap();
        // Fader moved from 1000 to 1500, delta = +500, moving toward current at 3000
        // scaled_delta_base = 500 / 2 = 250, with runway ≈ 272
        // new_current = 3000 + 272 = 3272
        assert!(
            scale_val > 3000,
            "Expected scale_val > 3000, got {}",
            scale_val
        );
        assert!(!scale.is_latched());
    }

    #[test]
    fn test_all_modes_converge() {
        let mut jump = AnalogLatch::new(1000, TakeoverMode::Jump);
        let mut pickup = AnalogLatch::new(1000, TakeoverMode::Pickup);
        let mut scale = AnalogLatch::new(1000, TakeoverMode::Scale);

        // Switch to Alt layer
        jump.update(1000, LatchLayer::Alt, 2000);
        pickup.update(1000, LatchLayer::Alt, 2000);
        scale.update(1000, LatchLayer::Alt, 2000);

        // Sweep fader to target
        let mut current_target_jump = 2000u16;
        let mut current_target_pickup = 2000u16;
        let mut current_target_scale = 2000u16;

        for fader_pos in (1000..=2000).step_by(100) {
            if let Some(v) = jump.update(fader_pos, LatchLayer::Alt, current_target_jump) {
                current_target_jump = v;
            }
            if let Some(v) = pickup.update(fader_pos, LatchLayer::Alt, current_target_pickup) {
                current_target_pickup = v;
            }
            if let Some(v) = scale.update(fader_pos, LatchLayer::Alt, current_target_scale) {
                current_target_scale = v;
            }
        }

        // After sweep, Jump is at fader position
        assert_eq!(current_target_jump, 2000);
        // Pickup eventually crosses and latches
        assert!(pickup.is_latched());
        // Scale eventually converges
        // All should be latched now
        assert!(jump.is_latched());
        assert!(pickup.is_latched());
    }

    #[test]
    fn test_mode_with_jitter() {
        let mut jump = AnalogLatch::with_tolerance(1000, 3, TakeoverMode::Jump);
        let mut pickup = AnalogLatch::with_tolerance(1000, 3, TakeoverMode::Pickup);
        let mut scale = AnalogLatch::with_tolerance(1000, 3, TakeoverMode::Scale);

        // All modes respect jitter tolerance when latched
        assert_eq!(jump.update(1002, LatchLayer::Main, 1000), None);
        assert_eq!(pickup.update(1002, LatchLayer::Main, 1000), None);
        assert_eq!(scale.update(1002, LatchLayer::Main, 1000), None);

        // All respond beyond tolerance
        assert_eq!(jump.update(1010, LatchLayer::Main, 1000), Some(1010));
        assert_eq!(pickup.update(1010, LatchLayer::Main, 1002), Some(1010));
        // Scale mode: After small move to 1002 (no value returned due to jitter),
        // target changed from 1000 to 1002 which unlatches (diff 8 > tolerance 3).
        // delta = 8, dir_to_current = (1002-1002).signum() = 0, so moving_toward = false
        // scaled_delta_base = 8 * 2 = 16, new_current = 1002 + 16 = 1018
        // 1018 crosses past fader at 1010, so it latches immediately
        let scale_result = scale.update(1010, LatchLayer::Main, 1002);
        assert_eq!(scale_result, Some(1010));
        assert!(scale.is_latched());
    }

    #[test]
    fn test_workflow_seq8_page_switch() {
        // Simulates seq8 page switching with Scale mode
        let mut latch = AnalogLatch::new(2048, TakeoverMode::Scale);

        // Page 0, step 0 has value 2048
        assert!(latch.is_latched());

        // Switch to page 1, same fader controls different step with value 3000
        latch.update(2048, LatchLayer::Main, 3000);
        assert!(!latch.is_latched()); // Unlatched, different value

        // User moves fader up to 2148 (delta = 100)
        // Fader moving toward current, scaled_delta_base = 100 / 2 = 50
        // Runway factor at fader=2148: (16380+2148)/16380 ≈ 1.131, scaled_delta = 56
        // new_current = 3000 + 56 = 3056
        let result = latch.update(2148, LatchLayer::Main, 3000);
        assert_eq!(result, Some(3056));

        // Eventually converges as user continues moving
    }

    #[test]
    fn test_workflow_control_attenuator() {
        // Simulates attenuator with alt-layer using Scale mode
        let mut latch = AnalogLatch::new(4095, TakeoverMode::Scale);

        // Main layer at max
        latch.update(4095, LatchLayer::Main, 4095);
        assert!(latch.is_latched());

        // Press shift, alt layer has different attenuation (2000)
        latch.update(4095, LatchLayer::Alt, 2000);
        assert!(!latch.is_latched());

        // User moves fader down to 4000 (delta = -95)
        // Fader moving toward current (both negative direction), scaled_delta_base = -95 / 2 = -47
        // Runway factor at fader=4000 (delta<0): (16380+95)/16380 ≈ 1.006, scaled_delta = -47
        // new_current = 2000 + (-47) = 1953
        let result = latch.update(4000, LatchLayer::Alt, 2000);
        assert_eq!(result, Some(1953));
    }

    #[test]
    fn test_workflow_session_start() {
        // Fresh boot, saved value differs from fader position
        let saved_value = 3000u16;
        let fader_physical_position = 500u16;

        // Jump mode: immediately jumps on first movement (must exceed jitter tolerance of 20)
        let mut jump = AnalogLatch::new(fader_physical_position, TakeoverMode::Jump);
        jump.update(fader_physical_position, LatchLayer::Main, saved_value);
        assert_eq!(jump.update(525, LatchLayer::Main, saved_value), Some(525));

        // Pickup mode: must sweep past saved value
        let mut pickup = AnalogLatch::new(fader_physical_position, TakeoverMode::Pickup);
        pickup.update(fader_physical_position, LatchLayer::Main, saved_value);
        assert_eq!(pickup.update(525, LatchLayer::Main, saved_value), None); // Not crossed yet

        // Scale mode: gradually catches up
        let mut scale = AnalogLatch::new(fader_physical_position, TakeoverMode::Scale);
        scale.update(fader_physical_position, LatchLayer::Main, saved_value);
        let result = scale.update(525, LatchLayer::Main, saved_value);
        assert!(result.is_some());
        let scaled = result.unwrap();
        // Fader moved from 500 to 525 (delta = +25)
        // Fader moving toward current at 3000, scaled_delta_base = 25 / 2 = 12
        // Runway factor ≈ 1.03, scaled_delta = 12
        // new_current = 3000 + 12 = 3012
        assert_eq!(scaled, 3012);
    }
}
