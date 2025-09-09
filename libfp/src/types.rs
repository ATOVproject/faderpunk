use serde::{Deserialize, Serialize};

use crate::{CALIBRATION_SCALE_FACTOR, CALIBRATION_VERSION_LATEST, CALIB_FILE_MAGIC};

// --- V1 (Old) Format Definition ---
#[derive(Deserialize, Default, Copy, Clone)]
pub struct MaxCalibrationV1 {
    pub inputs: [(f32, f32); 2],
    pub outputs: [[(f32, f32); 2]; 20],
}

// --- V2 (New) Format Definition ---
// This is the canonical format for the application.
pub type RegressionValues = (i64, i64);
pub type RegressionValuesInput = [RegressionValues; 2];
pub type RegressionValuesOutput = [[RegressionValues; 2]; 20];

#[derive(Serialize, Deserialize, Default, Copy, Clone)]
pub struct MaxCalibration {
    pub inputs: RegressionValuesInput,
    pub outputs: RegressionValuesOutput,
}

#[derive(Serialize, Deserialize, Copy, Clone)]
pub struct CalibFile {
    pub magic: [u8; 4],
    pub version: u8,
    pub data: MaxCalibration,
}

impl CalibFile {
    pub fn new(data: MaxCalibration) -> Self {
        Self {
            magic: CALIB_FILE_MAGIC,
            version: CALIBRATION_VERSION_LATEST,
            data,
        }
    }
}

// --- Migration Logic ---
// This function converts the old V1 data into the new V2 format.
impl From<MaxCalibrationV1> for MaxCalibration {
    fn from(old: MaxCalibrationV1) -> Self {
        let mut new = MaxCalibration::default();

        // Convert inputs
        for i in 0..old.inputs.len() {
            let (old_slope, old_intercept) = old.inputs[i];
            let new_slope_f32 = 1.0 + old_slope;
            new.inputs[i] = (
                (new_slope_f32 * CALIBRATION_SCALE_FACTOR as f32) as i64,
                (old_intercept * CALIBRATION_SCALE_FACTOR as f32) as i64,
            );
        }

        // Convert the old output coefficients to match the new firmware logic.
        for i in 0..old.outputs.len() {
            for j in 0..old.outputs[i].len() {
                let (slope_v1, intercept_v1) = old.outputs[i][j];
                let denominator = 1.0 + slope_v1;

                if denominator.abs() > f32::EPSILON {
                    // The old V1 data was based on: ideal = (1+s1)*raw + i1
                    // The new firmware needs:         raw = s2*ideal + i2
                    // By rearranging the first formula, we can find s2 and i2:
                    // raw = (1/(1+s1))*ideal - i1/(1+s1)

                    let slope_v2_f32 = 1.0 / denominator;
                    let intercept_v2_f32 = -intercept_v1 / denominator;

                    new.outputs[i][j] = (
                        (slope_v2_f32 * CALIBRATION_SCALE_FACTOR as f32) as i64,
                        (intercept_v2_f32 * CALIBRATION_SCALE_FACTOR as f32) as i64,
                    );
                }
            }
        }
        new
    }
}
