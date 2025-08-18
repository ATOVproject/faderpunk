/// [(slope, intercept) for 0-10V range, (slope, intercept) for -5-5V range]
pub type RegressionValuesInput = [(f32, f32); 2];
/// [[(slope, intercept) for 0-10V range, (slope, intercept) for -5-5V range]; 20]
pub type RegressionValuesOutput = [[(f32, f32); 2]; 20];
