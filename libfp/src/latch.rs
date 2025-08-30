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
}

impl AnalogLatch {
    /// Creates a new AnalogLatch.
    ///
    /// It starts on layer 0 and assumes the fader's initial physical position
    /// matches the a given initial value, so it begins in a "latched" state.
    pub fn new(initial_value: u16) -> Self {
        Self {
            // Default to layer 0 being active.
            active_layer: LatchLayer::Main,
            // Assume we are latched to the initial value.
            is_latched: true,
            // The fader's last known position is its starting position.
            prev_value: initial_value,
            // The initial target is the same as the initial value
            prev_target: initial_value,
        }
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
    /// * `new_active_layer_index`: The index of the layer that should be active.
    /// * `active_layer_target_value`: The currently stored value for the active layer. This is
    ///   used to detect the crossover point when the fader is not latched.
    ///
    /// # Returns
    /// * `Some(new_value)` if the fader is latched and its value has changed, or if the fader
    ///   has just crossed the target value and become latched. The caller should use this
    ///   new value to update their state for the active layer.
    /// * `None` if no change should occur (e.g., the fader is moving but has not yet
    ///   reached the target value).
    pub fn update(
        &mut self,
        value: u16,
        new_active_layer: LatchLayer,
        active_layer_target_value: u16,
    ) -> Option<u16> {
        // Did the user switch layers?
        if new_active_layer != self.active_layer {
            self.active_layer = new_active_layer;
            // Unlatch unless the fader is already at the new target value.
            self.is_latched = value == active_layer_target_value;
            self.prev_target = active_layer_target_value;
        } else if self.is_latched {
            // If we are latched but the target has changed externally, check if we should unlatch.
            // This happens if the target value is changed by something other than this fader.
            if self.prev_target != active_layer_target_value {
                // If the new target equals our current position, stay latched
                self.is_latched = value == active_layer_target_value;
                self.prev_target = active_layer_target_value;
            }
        } else {
            // If we are unlatched and the target changes to our current position, latch immediately
            if self.prev_target != active_layer_target_value && value == active_layer_target_value {
                self.is_latched = true;
                self.prev_target = active_layer_target_value;
            } else if self.prev_target != active_layer_target_value {
                self.prev_target = active_layer_target_value;
            }
        }

        let mut new_value = None;

        if self.is_latched {
            // Fader is in control. If it moves, the value changes.
            if value != self.prev_value {
                new_value = Some(value);
            }
        } else {
            // Fader is not in control. Check for crossover.
            let has_crossed = (self.prev_value..=value).contains(&active_layer_target_value)
                || (value..=self.prev_value).contains(&active_layer_target_value);

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
    fn test_crossover_from_target_position() {
        let mut latch = AnalogLatch::new(150);

        // Switch layers, fader starts at exact target value
        let result = latch.update(150, LatchLayer::Alt, 150);
        assert_eq!(result, None);
        // Should be latched since at target
        assert!(latch.is_latched());

        // Any movement should update value
        let result = latch.update(160, LatchLayer::Alt, 150);
        assert_eq!(result, Some(160));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_edge_case_zero_values() {
        let mut latch = AnalogLatch::new(0);

        // Switch layer and move from 0 to 50. The target for the new layer is 0.
        // The movement from prev_value(0) to value(50) crosses the target(0),
        // so it should latch immediately.
        let result = latch.update(50, LatchLayer::Alt, 0);
        assert_eq!(result, Some(50));
        assert!(latch.is_latched());

        // Cross back to zero
        let result = latch.update(0, LatchLayer::Alt, 0);
        assert_eq!(result, Some(0));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_edge_case_max_values() {
        let mut latch = AnalogLatch::new(u16::MAX);

        // Switch layer and move. The target is u16::MAX.
        // The movement from prev_value(MAX) to value(MAX-100) crosses the target(MAX),
        // so it should latch immediately
        let result = latch.update(u16::MAX - 100, LatchLayer::Alt, u16::MAX);
        assert_eq!(result, Some(u16::MAX - 100));
        assert!(latch.is_latched());

        // Cross back to max
        let result = latch.update(u16::MAX, LatchLayer::Alt, u16::MAX);
        assert_eq!(result, Some(u16::MAX));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_internal_target_change_while_latched() {
        let mut latch = AnalogLatch::new(100);
        assert!(latch.is_latched());

        // The target value for the active layer changes from 100 to 200 internally,
        // but the fader's physical position is still 100
        // No layer switch occurs
        let result = latch.update(100, LatchLayer::Main, 200);

        // The fader has not moved, so the result should be None
        assert_eq!(result, None);
        // The latch should now be unlatched because the physical position (100)
        // no longer matches the new target value (200)
        assert!(!latch.is_latched());

        // Move the fader towards the new target, but not past it
        let result = latch.update(150, LatchLayer::Main, 200);
        assert_eq!(result, None);
        assert!(!latch.is_latched());

        // Now, cross the new target value
        let result = latch.update(210, LatchLayer::Main, 200);
        assert_eq!(result, Some(210));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_multiple_movements_while_latched() {
        let mut latch = AnalogLatch::new(100);

        // Multiple movements should all stay latched
        assert_eq!(latch.update(120, LatchLayer::Main, 100), Some(120));
        assert!(latch.is_latched());

        assert_eq!(latch.update(150, LatchLayer::Main, 100), Some(150));
        assert!(latch.is_latched());

        assert_eq!(latch.update(80, LatchLayer::Main, 100), Some(80));
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
    fn test_multiple_target_changes_while_unlatched() {
        let mut latch = AnalogLatch::new(100);

        // Target changes externally
        assert_eq!(latch.update(100, LatchLayer::Main, 200), None);
        assert!(!latch.is_latched());

        // Target changes again before we reach it
        assert_eq!(latch.update(100, LatchLayer::Main, 150), None);
        assert!(!latch.is_latched());

        // Move toward new target
        assert_eq!(latch.update(140, LatchLayer::Main, 150), None);
        assert!(!latch.is_latched());

        // Cross it
        assert_eq!(latch.update(160, LatchLayer::Main, 150), Some(160));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_return_to_original_target_after_external_change() {
        let mut latch = AnalogLatch::new(100);

        // Move fader away
        assert_eq!(latch.update(150, LatchLayer::Main, 100), Some(150));
        assert!(latch.is_latched());

        // Target changes externally to 200, causing unlatch
        assert_eq!(latch.update(150, LatchLayer::Main, 200), None);
        assert!(!latch.is_latched());

        // Move back toward original position (100)
        // but target is still 200, so no latch
        assert_eq!(latch.update(100, LatchLayer::Main, 200), None);
        assert!(!latch.is_latched());
    }

    #[test]
    fn test_no_movement_crossover() {
        let mut latch = AnalogLatch::new(100);

        // Target changes to exactly where we are
        // Should latch immediately
        assert_eq!(latch.update(100, LatchLayer::Main, 100), None);
        assert!(latch.is_latched());

        // Unlatch by external change
        assert_eq!(latch.update(100, LatchLayer::Main, 200), None);
        assert!(!latch.is_latched());

        // Target changes back to our position
        // Should latch immediately even without movement
        assert_eq!(latch.update(100, LatchLayer::Main, 100), None);
        assert!(latch.is_latched());
    }
}
