import type { GlobalConfig } from "@atov/fp-config";
import { useCallback, useState } from "react";
import {
  Modal,
  ModalBody,
  ModalContent,
  ModalFooter,
  ModalHeader,
} from "@heroui/modal";
import { Input } from "@heroui/input";
import { Select, SelectItem } from "@heroui/select";

import { useStore } from "../../store";
import { setGlobalConfig } from "../../utils/config";
import { sendAndReceive } from "../../utils/usb-protocol";
import { ButtonPrimary, ButtonSecondary } from "../Button";

interface Props {
  config: GlobalConfig;
}

const LOW_DAC_COUNTS = 410; // 1V
const HIGH_DAC_COUNTS = 1638; // 4V
const OCTAVE_SPAN = 3.0; // 4V - 1V at standard 1V/oct

type WizardMode = "auto" | "manual";

type WizardStep =
  | { type: "setup" }
  | { type: "measuring1" }
  | { type: "measuring2"; f1: number }
  | { type: "measuring_confirm"; countsPerOct: number; f1: number }
  | { type: "manual_enter1" }
  | { type: "manual_enter2"; f1: number }
  | {
      type: "confirm";
      countsPerOct: number;
      f1: number;
      fConfirm: number | null;
    }
  | { type: "error"; message: string };

const CURVE_LABELS = ["Custom 1", "Custom 2", "Custom 3", "Custom 4"] as const;

const JACK_OPTIONS = Array.from({ length: 16 }, (_, i) => ({
  key: String(i),
  label: `Jack ${i + 1}`,
}));

const AUX_OPTIONS = [
  { key: "0", label: "Atom" },
  { key: "1", label: "Meteor" },
  { key: "2", label: "Cube" },
];

const formatGain = (countsPerOct: number): string =>
  `${(countsPerOct / 410).toFixed(3)} V/Oct`;

const parseFreq = (s: string): number | null => {
  const n = parseFloat(s);
  return isFinite(n) && n >= 10 && n <= 20000 ? n : null;
};

export const VoOctCurvesSettings = ({ config }: Props) => {
  const { usbDevice, isSimulator, setConfig } = useStore();

  const [openCurveIdx, setOpenCurveIdx] = useState<number | null>(null);
  const [wizardMode, setWizardMode] = useState<WizardMode>("auto");
  const [outputJack, setOutputJack] = useState("0");
  const [auxInput, setAuxInput] = useState("0");
  const [wizardStep, setWizardStep] = useState<WizardStep>({ type: "setup" });
  const [manualF1, setManualF1] = useState("");
  const [manualF2, setManualF2] = useState("");

  const handleOpenWizard = useCallback((idx: number) => {
    setOpenCurveIdx(idx);
    setWizardStep({ type: "setup" });
    setWizardMode("auto");
    setManualF1("");
    setManualF2("");
  }, []);

  const handleClose = useCallback(() => {
    // Release output jack if manual mode is holding a voltage. Fire-and-forget
    // so the modal closes immediately while cleanup runs in the background.
    if (
      (wizardStep.type === "manual_enter1" ||
        wizardStep.type === "manual_enter2") &&
      usbDevice
    ) {
      void sendAndReceive(usbDevice, {
        tag: "ReleaseVoOctOutput",
        value: { output_jack: parseInt(outputJack, 10) },
      }).catch(() => {});
    }
    setOpenCurveIdx(null);
  }, [wizardStep, usbDevice, outputJack]);

  // --- Automated flow ---

  const handleStartAuto = useCallback(async () => {
    if (!usbDevice || openCurveIdx === null) return;

    const jack = parseInt(outputJack, 10);
    const aux = parseInt(auxInput, 10);
    if (!Number.isFinite(jack) || !Number.isFinite(aux)) return;

    setWizardStep({ type: "measuring1" });
    let r1;
    try {
      r1 = await sendAndReceive(usbDevice, {
        tag: "MeasureVoOct",
        value: {
          output_jack: jack,
          aux_input: aux,
          dac_counts: LOW_DAC_COUNTS,
        },
      });
    } catch (e) {
      setWizardStep({ type: "error", message: String(e) });
      return;
    }
    if (r1.tag !== "VoOctFrequency") {
      setWizardStep({
        type: "error",
        message:
          "No signal detected at 1V. Check AUX jack and VCO connections.",
      });
      return;
    }
    const f1 = r1.value.freq_hz;

    setWizardStep({ type: "measuring2", f1 });
    let r2;
    try {
      r2 = await sendAndReceive(usbDevice, {
        tag: "MeasureVoOct",
        value: {
          output_jack: jack,
          aux_input: aux,
          dac_counts: HIGH_DAC_COUNTS,
        },
      });
    } catch (e) {
      setWizardStep({ type: "error", message: String(e) });
      return;
    }
    if (r2.tag !== "VoOctFrequency") {
      setWizardStep({
        type: "error",
        message:
          "No signal detected at 4V. Check AUX jack and VCO connections.",
      });
      return;
    }
    const f2 = r2.value.freq_hz;

    const countsPerOct = Math.round((OCTAVE_SPAN / Math.log2(f2 / f1)) * 410);

    if (countsPerOct <= 0 || countsPerOct > 60000) {
      setWizardStep({
        type: "error",
        message:
          countsPerOct <= 0
            ? `VCO frequency went down from 1V to 4V (${f1.toFixed(1)} Hz → ${f2.toFixed(1)} Hz). Check output jack and VCO V/Oct wiring.`
            : `Calculated gain (${countsPerOct} counts/oct) is out of range. Check connections.`,
      });
      return;
    }

    // Confirm by outputting one countsPerOct above LOW (1V), staying within the
    // 1–4V calibration range. Pre-set the output now via SetVoOctOutput so the
    // VCO can settle downward from 4V during the 800 ms UI wait, before the
    // firmware's own settle window starts.
    const confirmDacCounts = LOW_DAC_COUNTS + countsPerOct;
    if (confirmDacCounts > 65535) {
      setWizardStep({ type: "confirm", countsPerOct, f1, fConfirm: null });
      return;
    }

    setWizardStep({ type: "measuring_confirm", countsPerOct, f1 });

    // Pre-settle: bring the output to confirmDacCounts now. Errors are ignored;
    // MeasureVoOct will reconfigure the port regardless.
    try {
      await sendAndReceive(usbDevice, {
        tag: "SetVoOctOutput",
        value: { output_jack: jack, dac_counts: confirmDacCounts },
      });
    } catch {
      // ignore — MeasureVoOct below will configure the port itself
    }
    // Give the VCO extra time to settle downward before the firmware window.
    await new Promise<void>((r) => setTimeout(r, 800));

    let rConfirm;
    try {
      rConfirm = await sendAndReceive(usbDevice, {
        tag: "MeasureVoOct",
        value: {
          output_jack: jack,
          aux_input: aux,
          dac_counts: confirmDacCounts,
        },
      });
    } catch (e) {
      setWizardStep({ type: "error", message: String(e) });
      return;
    }
    if (rConfirm.tag !== "VoOctFrequency") {
      setWizardStep({
        type: "error",
        message: "Verification measurement failed. Check AUX jack connection.",
      });
      return;
    }

    setWizardStep({
      type: "confirm",
      countsPerOct,
      f1,
      fConfirm: rConfirm.value.freq_hz,
    });
  }, [usbDevice, openCurveIdx, outputJack, auxInput]);

  // --- Manual flow ---

  const handleStartManual = useCallback(async () => {
    if (!usbDevice || openCurveIdx === null) return;

    const jack = parseInt(outputJack, 10);
    try {
      const r = await sendAndReceive(usbDevice, {
        tag: "SetVoOctOutput",
        value: { output_jack: jack, dac_counts: LOW_DAC_COUNTS },
      });
      if (r.tag !== "VoOctOutputSet") throw new Error("unexpected response");
    } catch (e) {
      setWizardStep({ type: "error", message: String(e) });
      return;
    }
    setManualF1("");
    setWizardStep({ type: "manual_enter1" });
  }, [usbDevice, openCurveIdx, outputJack]);

  const handleManualNext = useCallback(async () => {
    if (!usbDevice) return;
    const f1 = parseFreq(manualF1);
    if (f1 === null) return;

    const jack = parseInt(outputJack, 10);
    try {
      const r = await sendAndReceive(usbDevice, {
        tag: "SetVoOctOutput",
        value: { output_jack: jack, dac_counts: HIGH_DAC_COUNTS },
      });
      if (r.tag !== "VoOctOutputSet") throw new Error("unexpected response");
    } catch (e) {
      setWizardStep({ type: "error", message: String(e) });
      return;
    }
    setManualF2("");
    setWizardStep({ type: "manual_enter2", f1 });
  }, [usbDevice, outputJack, manualF1]);

  const handleManualCalculate = useCallback(async () => {
    if (!usbDevice || wizardStep.type !== "manual_enter2") return;
    const f2 = parseFreq(manualF2);
    if (f2 === null) return;

    const { f1 } = wizardStep;
    try {
      await sendAndReceive(usbDevice, {
        tag: "ReleaseVoOctOutput",
        value: { output_jack: parseInt(outputJack, 10) },
      });
    } catch (e) {
      setWizardStep({ type: "error", message: String(e) });
      return;
    }

    const countsPerOct = Math.round((OCTAVE_SPAN / Math.log2(f2 / f1)) * 410);
    setWizardStep({ type: "confirm", countsPerOct, f1, fConfirm: null });
  }, [usbDevice, wizardStep, manualF2, outputJack]);

  // --- Save ---

  const handleSave = useCallback(async () => {
    if (openCurveIdx === null || wizardStep.type !== "confirm") return;

    const updatedCurves = [
      ...config.custom_voct_curves,
    ] as unknown as typeof config.custom_voct_curves;
    updatedCurves[openCurveIdx] = { counts_per_oct: wizardStep.countsPerOct };
    const updatedConfig: GlobalConfig = {
      ...config,
      custom_voct_curves: updatedCurves,
    };

    if (usbDevice && !isSimulator) {
      await setGlobalConfig(usbDevice, updatedConfig);
    }
    setConfig(updatedConfig);
    setOpenCurveIdx(null);
  }, [openCurveIdx, wizardStep, config, usbDevice, isSimulator, setConfig]);

  // -------------------- render --------------------

  const isMeasuring =
    wizardStep.type === "measuring1" ||
    wizardStep.type === "measuring2" ||
    wizardStep.type === "measuring_confirm";

  return (
    <div className="mb-12">
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
        Custom V/Oct Curves
      </h2>
      <div className="flex flex-col gap-y-4 px-4">
        {CURVE_LABELS.map((label, idx) => {
          const cpo = config.custom_voct_curves[idx]?.counts_per_oct ?? 0;
          return (
            <div key={idx} className="flex items-center justify-between">
              <span className="text-sm font-medium">{label}</span>
              <span className="text-sm text-gray-400">
                {cpo === 0 ? "Not calibrated" : formatGain(cpo)}
              </span>
              <ButtonSecondary
                size="sm"
                onPress={() => handleOpenWizard(idx)}
                isDisabled={!usbDevice || isSimulator}
              >
                Calibrate
              </ButtonSecondary>
            </div>
          );
        })}
      </div>

      <Modal
        isOpen={openCurveIdx !== null}
        onOpenChange={(open) => {
          if (!open) handleClose();
        }}
      >
        <ModalContent>
          {(onClose) => (
            <>
              <ModalHeader>
                Calibrate{" "}
                {openCurveIdx !== null ? CURVE_LABELS[openCurveIdx] : ""}
              </ModalHeader>

              <ModalBody>
                {wizardStep.type === "setup" && (
                  <div className="flex flex-col gap-y-4">
                    <div className="flex overflow-hidden rounded-lg border border-gray-700">
                      <button
                        type="button"
                        onClick={() => setWizardMode("auto")}
                        className={`flex-1 py-1.5 text-sm font-medium transition-colors ${
                          wizardMode === "auto"
                            ? "bg-yellow-fp text-black"
                            : "text-gray-400 hover:text-white"
                        }`}
                      >
                        Automated
                      </button>
                      <button
                        type="button"
                        onClick={() => setWizardMode("manual")}
                        className={`flex-1 py-1.5 text-sm font-medium transition-colors ${
                          wizardMode === "manual"
                            ? "bg-yellow-fp text-black"
                            : "text-gray-400 hover:text-white"
                        }`}
                      >
                        Manual
                      </button>
                    </div>

                    <Select
                      label="Output jack (to VCO V/Oct)"
                      selectedKeys={[outputJack]}
                      disallowEmptySelection
                      onSelectionChange={(keys) => {
                        const first = [...keys][0];
                        if (
                          typeof first === "string" ||
                          typeof first === "number"
                        )
                          setOutputJack(String(first));
                      }}
                      items={JACK_OPTIONS}
                    >
                      {(item) => (
                        <SelectItem key={item.key}>{item.label}</SelectItem>
                      )}
                    </Select>

                    {wizardMode === "auto" ? (
                      <>
                        <Select
                          label="AUX input (from VCO audio out)"
                          selectedKeys={[auxInput]}
                          disallowEmptySelection
                          onSelectionChange={(keys) => {
                            const first = [...keys][0];
                            if (
                              typeof first === "string" ||
                              typeof first === "number"
                            )
                              setAuxInput(String(first));
                          }}
                          items={AUX_OPTIONS}
                        >
                          {(item) => (
                            <SelectItem key={item.key}>{item.label}</SelectItem>
                          )}
                        </Select>
                        <p className="text-xs text-gray-400">
                          Faderpunk will output 1V and 4V and measure the VCO
                          frequency automatically via the AUX jack.
                        </p>
                      </>
                    ) : (
                      <p className="text-xs text-gray-400">
                        Faderpunk will output 1V and 4V. Read the frequency from
                        an external tuner and enter it manually.
                      </p>
                    )}
                  </div>
                )}

                {wizardStep.type === "measuring1" && (
                  <div className="flex flex-col gap-y-2">
                    <p className="animate-pulse text-sm">
                      Outputting 1V — measuring frequency…
                    </p>
                    <p className="text-xs text-gray-400">
                      Make sure your VCO is producing audio on the selected AUX
                      jack.
                    </p>
                  </div>
                )}

                {wizardStep.type === "measuring2" && (
                  <div className="flex flex-col gap-y-2">
                    <p className="text-sm text-gray-400">
                      1V: {wizardStep.f1.toFixed(1)} Hz ✓
                    </p>
                    <p className="animate-pulse text-sm">
                      Outputting 4V — measuring frequency…
                    </p>
                  </div>
                )}

                {wizardStep.type === "measuring_confirm" && (
                  <div className="flex flex-col gap-y-2">
                    <p className="text-sm text-gray-400">
                      1V: {wizardStep.f1.toFixed(1)} Hz ✓
                    </p>
                    <p className="text-sm text-gray-400">
                      Gain: {formatGain(wizardStep.countsPerOct)} ✓
                    </p>
                    <p className="animate-pulse text-sm">
                      Verifying 1 octave above 1V…
                    </p>
                  </div>
                )}

                {wizardStep.type === "manual_enter1" && (
                  <div className="flex flex-col gap-y-4">
                    <p className="text-sm">
                      Outputting 1V on Jack {parseInt(outputJack, 10) + 1}. Read
                      the frequency from your tuner and enter it below.
                    </p>
                    <Input
                      type="number"
                      label="Frequency at 1V (Hz)"
                      placeholder="e.g. 261.6"
                      value={manualF1}
                      onValueChange={setManualF1}
                      min={10}
                      max={20000}
                      step={0.1}
                    />
                  </div>
                )}

                {wizardStep.type === "manual_enter2" && (
                  <div className="flex flex-col gap-y-4">
                    <p className="text-sm text-gray-400">
                      1V: {wizardStep.f1.toFixed(1)} Hz ✓
                    </p>
                    <p className="text-sm">
                      Now outputting 4V. Read the new frequency from your tuner
                      and enter it below.
                    </p>
                    <Input
                      type="number"
                      label="Frequency at 4V (Hz)"
                      placeholder="e.g. 2093.0"
                      value={manualF2}
                      onValueChange={setManualF2}
                      min={10}
                      max={20000}
                      step={0.1}
                    />
                  </div>
                )}

                {wizardStep.type === "confirm" &&
                  (() => {
                    const { countsPerOct, f1, fConfirm } = wizardStep;
                    return (
                      <div className="flex flex-col gap-y-3">
                        <div className="flex flex-col gap-y-1">
                          <p className="text-sm font-medium">
                            Gain: {formatGain(countsPerOct)}
                          </p>
                          <p className="text-xs text-gray-400">
                            ({countsPerOct} counts/oct)
                          </p>
                        </div>
                        {fConfirm !== null &&
                          (() => {
                            const expected = f1 * 2;
                            const cents = Math.round(
                              1200 * Math.log2(fConfirm / expected),
                            );
                            const absCents = Math.abs(cents);
                            const sign = cents >= 0 ? "+" : "";
                            const quality =
                              absCents < 5
                                ? { label: "accurate", color: "text-green-400" }
                                : absCents < 20
                                  ? {
                                      label: "acceptable",
                                      color: "text-yellow-400",
                                    }
                                  : {
                                      label: "off — consider recalibrating",
                                      color: "text-red-400",
                                    };
                            return (
                              <div className="flex flex-col gap-y-1">
                                <p className="text-xs text-gray-400">
                                  Expected 1 octave above 1V:{" "}
                                  {expected.toFixed(1)} Hz
                                </p>
                                <p className="text-xs text-gray-400">
                                  Measured: {fConfirm.toFixed(1)} Hz
                                </p>
                                <p
                                  className={`text-sm font-medium ${quality.color}`}
                                >
                                  {sign}
                                  {cents} cents — {quality.label}
                                </p>
                              </div>
                            );
                          })()}
                      </div>
                    );
                  })()}

                {wizardStep.type === "error" && (
                  <p className="text-sm text-red-400">{wizardStep.message}</p>
                )}
              </ModalBody>

              <ModalFooter>
                {wizardStep.type === "setup" && (
                  <>
                    <ButtonPrimary
                      onPress={
                        wizardMode === "auto"
                          ? handleStartAuto
                          : handleStartManual
                      }
                    >
                      Start
                    </ButtonPrimary>
                    <ButtonSecondary onPress={onClose}>Cancel</ButtonSecondary>
                  </>
                )}

                {isMeasuring && (
                  <ButtonSecondary onPress={onClose}>Cancel</ButtonSecondary>
                )}

                {wizardStep.type === "manual_enter1" && (
                  <>
                    <ButtonPrimary
                      onPress={handleManualNext}
                      isDisabled={parseFreq(manualF1) === null}
                    >
                      Next
                    </ButtonPrimary>
                    <ButtonSecondary onPress={handleClose}>
                      Cancel
                    </ButtonSecondary>
                  </>
                )}

                {wizardStep.type === "manual_enter2" && (
                  <>
                    <ButtonPrimary
                      onPress={handleManualCalculate}
                      isDisabled={parseFreq(manualF2) === null}
                    >
                      Calculate
                    </ButtonPrimary>
                    <ButtonSecondary onPress={handleClose}>
                      Cancel
                    </ButtonSecondary>
                  </>
                )}

                {wizardStep.type === "confirm" && (
                  <>
                    <ButtonPrimary onPress={handleSave}>Save</ButtonPrimary>
                    <ButtonSecondary
                      onPress={() => setWizardStep({ type: "setup" })}
                    >
                      Recalibrate
                    </ButtonSecondary>
                  </>
                )}

                {wizardStep.type === "error" && (
                  <>
                    <ButtonPrimary
                      onPress={() => setWizardStep({ type: "setup" })}
                    >
                      Retry
                    </ButtonPrimary>
                    <ButtonSecondary onPress={onClose}>Cancel</ButtonSecondary>
                  </>
                )}
              </ModalFooter>
            </>
          )}
        </ModalContent>
      </Modal>
    </div>
  );
};
