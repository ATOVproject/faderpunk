/**
 * Browser entry: exposes window.FpMidi for the vanilla index.html script.
 */
import { pullSetupFromDevice, pushSetupToDevice } from "./setup-io.js";

window.FpMidi = {
  ready: true,
  pullSetupFromDevice,
  pushSetupToDevice,
};

window.dispatchEvent(new Event("fp-midi-ready"));
