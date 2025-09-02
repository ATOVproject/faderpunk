#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum LatchLayer {
    Main,
    Alt,
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
}

impl AnalogLatch {
    /// Creates a new AnalogLatch with default jitter tolerance.
    ///
    /// It starts on layer 0 and assumes the fader's initial physical position
    /// matches the a given initial value, so it begins in a "latched" state.
    pub fn new(initial_value: u16) -> Self {
        Self::with_tolerance(initial_value, 3) // Default tolerance of 3
    }

    /// Creates a new AnalogLatch with custom jitter tolerance.
    ///
    /// # Arguments
    /// * `initial_value`: The starting position of the fader
    /// * `jitter_tolerance`: The tolerance for considering values equal (to handle ADC noise)
    pub fn with_tolerance(initial_value: u16, jitter_tolerance: u16) -> Self {
        Self {
            active_layer: LatchLayer::Main,
            is_latched: true,
            prev_value: initial_value,
            prev_target: initial_value,
            jitter_tolerance,
        }
    }

    /// Checks if two values are approximately equal within the jitter tolerance
    fn values_equal(&self, a: u16, b: u16) -> bool {
        let diff = if a > b { a - b } else { b - a };
        diff <= self.jitter_tolerance
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
            // Unlatch unless the fader is already at the new target value (within tolerance)
            self.is_latched = self.values_equal(value, active_layer_target_value);
            self.prev_target = active_layer_target_value;
        } else if self.is_latched {
            // If we are latched but the target has changed externally, check if we should unlatch.
            // This happens if the target value is changed by something other than this fader.
            if self.prev_target != active_layer_target_value {
                // If the new target equals our current position (within tolerance), stay latched
                self.is_latched = self.values_equal(value, active_layer_target_value);
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

        if self.is_latched {
            // Fader is in control. If it moves beyond jitter tolerance, the value changes.
            if !self.values_equal(value, self.prev_value) {
                new_value = Some(value);
            }
        } else {
            // Fader is not in control. Check for crossover.
            // We consider it crossed if we've passed through or reached the target
            let has_crossed = (self.prev_value..=value).contains(&active_layer_target_value)
                || (value..=self.prev_value).contains(&active_layer_target_value)
                || self.values_equal(value, active_layer_target_value);

            if has_crossed {
                // Crossover detected! Latch and report the new value.
                self.is_latched = true;
                new_value = Some(value);
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
        let latch = AnalogLatch::new(100);
        assert_eq!(latch.active_layer(), LatchLayer::Main);
        assert!(latch.is_latched());
    }

    #[test]
    fn test_basic_latched_movement() {
        let mut latch = AnalogLatch::new(100);

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
        let mut latch = AnalogLatch::with_tolerance(100, 3);

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
        let mut latch = AnalogLatch::with_tolerance(100, 3);

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
        let mut latch = AnalogLatch::with_tolerance(100, 3);

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
        let mut latch = AnalogLatch::new(100);

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
        let mut latch = AnalogLatch::new(100);

        // Switch to layer 1, fader not at target value
        let result = latch.update(100, LatchLayer::Alt, 200);
        assert_eq!(result, None);
        assert_eq!(latch.active_layer(), LatchLayer::Alt);
        // Should become unlatched
        assert!(!latch.is_latched());
    }

    #[test]
    fn test_crossover_detection_upward() {
        let mut latch = AnalogLatch::new(100);

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
        let mut latch = AnalogLatch::with_tolerance(100, 3);

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
        let mut latch = AnalogLatch::new(200);

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
        let mut latch = AnalogLatch::new(100);

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
        let mut latch = AnalogLatch::new(100);

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
        let mut latch = AnalogLatch::new(100);

        // Switch layers and unlatch
        latch.update(100, LatchLayer::Alt, 200);
        assert!(!latch.is_latched());

        // Multiple movements without crossing target
        let result = latch.update(120, LatchLayer::Alt, 200);
        assert_eq!(result, None);
        assert!(!latch.is_latched());

        let result = latch.update(180, LatchLayer::Alt, 200);
        assert_eq!(result, None);
        assert!(!latch.is_latched());

        // Finally cross the target
        let result = latch.update(220, LatchLayer::Alt, 200);
        assert_eq!(result, Some(220));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_target_changes_to_fader_position() {
        let mut latch = AnalogLatch::new(100);

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
        let mut latch = AnalogLatch::with_tolerance(100, 3);

        // Move fader to 150
        assert_eq!(latch.update(150, LatchLayer::Main, 100), Some(150));
        assert!(latch.is_latched());

        // Target externally changes to within tolerance of fader position
        // Should stay latched
        assert_eq!(latch.update(150, LatchLayer::Main, 152), None);
        assert!(latch.is_latched());
    }
}
