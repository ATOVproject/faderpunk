/// A stateless machine that implements "catch-up" or "pickup" logic for a fader or knob.
///
/// This struct determines when a physical fader should take control of a value.
/// It is "stateless" in the sense that it does not store the values of the layers it manages.
/// The caller is responsible for maintaining the state and passing the relevant value to the `update` method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalogLatch {
    active_layer_index: usize,
    is_latched: bool,
    prev_value: u16,
}

impl AnalogLatch {
    /// Creates a new AnalogLatch.
    ///
    /// It starts on layer 0 and assumes the fader's initial physical position
    /// matches the a given initial value, so it begins in a "latched" state.
    pub fn new(initial_value: u16) -> Self {
        Self {
            // Default to layer 0 being active.
            active_layer_index: 0,
            // Assume we are latched to the initial value.
            is_latched: true,
            // The fader's last known position is its starting position.
            prev_value: initial_value,
        }
    }

    /// Returns the index of the layer that the latch is currently focused on.
    pub fn active_layer(&self) -> usize {
        self.active_layer_index
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
        new_active_layer_index: usize,
        active_layer_target_value: u16,
    ) -> Option<u16> {
        // Did the user switch layers?
        if new_active_layer_index != self.active_layer_index {
            self.active_layer_index = new_active_layer_index;
            // Unlatch unless the fader is already at the new target value.
            self.is_latched = value == active_layer_target_value;
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
        assert_eq!(latch.active_layer(), 0);
        assert!(latch.is_latched());
    }

    #[test]
    fn test_basic_latched_movement() {
        let mut latch = AnalogLatch::new(100);

        // Moving fader while latched should update value
        let result = latch.update(150, 0, 100);
        assert_eq!(result, Some(150));
        assert!(latch.is_latched());

        // No movement should return None
        let result = latch.update(150, 0, 100);
        assert_eq!(result, None);
        assert!(latch.is_latched());
    }

    #[test]
    fn test_layer_switching_exact_match() {
        let mut latch = AnalogLatch::new(100);

        // Switch to layer 1, fader moves to the new target value
        let result = latch.update(200, 1, 200);
        // The fader's physical value changed, so the change should be reported.
        assert_eq!(result, Some(200));
        assert_eq!(latch.active_layer(), 1);
        assert!(latch.is_latched()); // Should remain latched
    }

    #[test]
    fn test_layer_switching_different_value() {
        let mut latch = AnalogLatch::new(100);

        // Switch to layer 1, fader not at target value
        let result = latch.update(100, 1, 200);
        assert_eq!(result, None);
        assert_eq!(latch.active_layer(), 1);
        assert!(!latch.is_latched()); // Should become unlatched
    }

    #[test]
    fn test_crossover_detection_upward() {
        let mut latch = AnalogLatch::new(100);

        // Switch layers and unlatch
        latch.update(100, 1, 150);
        assert!(!latch.is_latched());

        // Move fader upward past target
        let result = latch.update(160, 1, 150);
        assert_eq!(result, Some(160));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_crossover_detection_downward() {
        let mut latch = AnalogLatch::new(200);

        // Switch layers and unlatch
        latch.update(200, 1, 150);
        assert!(!latch.is_latched());

        // Move fader downward past target
        let result = latch.update(140, 1, 150);
        assert_eq!(result, Some(140));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_crossover_detection_exact_hit() {
        let mut latch = AnalogLatch::new(100);

        // Switch layers and unlatch
        latch.update(100, 1, 150);
        assert!(!latch.is_latched());

        // Move fader to exact target value
        let result = latch.update(150, 1, 150);
        assert_eq!(result, Some(150));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_no_crossover_before_target() {
        let mut latch = AnalogLatch::new(100);

        // Switch layers and unlatch
        latch.update(100, 1, 200);
        assert!(!latch.is_latched());

        // Move fader but not past target
        let result = latch.update(150, 1, 200);
        assert_eq!(result, None);
        assert!(!latch.is_latched());
    }

    #[test]
    fn test_multiple_movements_unlatched() {
        let mut latch = AnalogLatch::new(100);

        // Switch layers and unlatch
        latch.update(100, 1, 200);
        assert!(!latch.is_latched());

        // Multiple movements without crossing target
        let result = latch.update(120, 1, 200);
        assert_eq!(result, None);
        assert!(!latch.is_latched());

        let result = latch.update(180, 1, 200);
        assert_eq!(result, None);
        assert!(!latch.is_latched());

        // Finally cross the target
        let result = latch.update(220, 1, 200);
        assert_eq!(result, Some(220));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_crossover_from_target_position() {
        let mut latch = AnalogLatch::new(150);

        // Switch layers, fader starts at exact target value
        let result = latch.update(150, 1, 150);
        assert_eq!(result, None);
        assert!(latch.is_latched()); // Should be latched since at target

        // Any movement should update value
        let result = latch.update(160, 1, 150);
        assert_eq!(result, Some(160));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_edge_case_zero_values() {
        let mut latch = AnalogLatch::new(0);

        // Switch layer and move from 0 to 50. The target for the new layer is 0.
        // The movement from prev_value(0) to value(50) crosses the target(0),
        // so it should latch immediately.
        let result = latch.update(50, 1, 0);
        assert_eq!(result, Some(50));
        assert!(latch.is_latched());

        // Cross back to zero
        let result = latch.update(0, 1, 0);
        assert_eq!(result, Some(0));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_edge_case_max_values() {
        let mut latch = AnalogLatch::new(u16::MAX);

        // Switch layer and move. The target is u16::MAX.
        // The movement from prev_value(MAX) to value(MAX-100) crosses the target(MAX),
        // so it should latch immediately.
        let result = latch.update(u16::MAX - 100, 1, u16::MAX);
        assert_eq!(result, Some(u16::MAX - 100));
        assert!(latch.is_latched());

        // Cross back to max
        let result = latch.update(u16::MAX, 1, u16::MAX);
        assert_eq!(result, Some(u16::MAX));
        assert!(latch.is_latched());
    }

    #[test]
    fn test_rapid_layer_switching() {
        let mut latch = AnalogLatch::new(100);

        // Rapid layer switches
        latch.update(100, 1, 200);
        assert_eq!(latch.active_layer(), 1);
        assert!(!latch.is_latched());

        latch.update(100, 2, 100);
        assert_eq!(latch.active_layer(), 2);
        assert!(latch.is_latched()); // Back to exact match

        latch.update(100, 0, 150);
        assert_eq!(latch.active_layer(), 0);
        assert!(!latch.is_latched());
    }
}
