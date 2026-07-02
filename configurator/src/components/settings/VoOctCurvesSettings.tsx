import type { GlobalConfig } from "@atov/fp-config";
import { useCallback, useState } from "react";
import { Input } from "@heroui/input";
import {
  Modal,
  ModalBody,
  ModalContent,
  ModalFooter,
  ModalHeader,
} from "@heroui/modal";
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

type WizardStep =
  | { type: "setup" }
  | { type: "step1" }
  | { type: "step2" }
  | { type: "done"; countsPerOct: number; gain: number }
  | { type: "error"; message: string };

const CURVE_LABELS = ["Custom 1", "Custom 2", "Custom 3", "Custom 4"] as const;

const JACK_OPTIONS = Array.from({ length: 16 }, (_, i) => ({
  key: String(i),
  label: `Jack ${i + 1}`,
}));

const formatGain = (countsPerOct: number): string =>
  `${(countsPerOct / 410).toFixed(3)} V/Oct`;

export const VoOctCurvesSettings = ({ config }: Props) => {
  const { usbDevice, isSimulator, setConfig } = useStore();

  const [openCurveIdx, setOpenCurveIdx] = useState<number | null>(null);
  const [outputJack, setOutputJack] = useState("0");
  const [wizardStep, setWizardStep] = useState<WizardStep>({ type: "setup" });
  const [freqInput, setFreqInput] = useState("");
  const [f1, setF1] = useState<number | null>(null);

  const releaseOutput = useCallback(
    (jack: string) => {
      if (!usbDevice) return;
      sendAndReceive(usbDevice, {
        tag: "ReleaseVoOctOutput",
        value: { output_jack: parseInt(jack, 10) },
      }).catch(() => {});
    },
    [usbDevice],
  );

  const handleOpenWizard = useCallback((idx: number) => {
    setOpenCurveIdx(idx);
    setWizardStep({ type: "setup" });
    setFreqInput("");
    setF1(null);
  }, []);

  const handleClose = useCallback(() => {
    if (wizardStep.type === "step1" || wizardStep.type === "step2") {
      releaseOutput(outputJack);
    }
    setOpenCurveIdx(null);
  }, [wizardStep, outputJack, releaseOutput]);

  const handleStart = useCallback(async () => {
    if (!usbDevice || openCurveIdx === null) return;
    try {
      const r = await sendAndReceive(usbDevice, {
        tag: "SetVoOctOutput",
        value: {
          output_jack: parseInt(outputJack, 10),
          dac_counts: LOW_DAC_COUNTS,
        },
      });
      if (r.tag !== "VoOctOutputSet") {
        setWizardStep({ type: "error", message: "Failed to set output." });
        return;
      }
      setFreqInput("");
      setWizardStep({ type: "step1" });
    } catch (e) {
      setWizardStep({ type: "error", message: String(e) });
    }
  }, [usbDevice, openCurveIdx, outputJack]);

  const handleStep1Next = useCallback(async () => {
    if (!usbDevice) return;
    const f1Val = parseFloat(freqInput);
    if (!Number.isFinite(f1Val) || f1Val <= 0) return;

    try {
      const r = await sendAndReceive(usbDevice, {
        tag: "SetVoOctOutput",
        value: {
          output_jack: parseInt(outputJack, 10),
          dac_counts: HIGH_DAC_COUNTS,
        },
      });
      if (r.tag !== "VoOctOutputSet") {
        setWizardStep({ type: "error", message: "Failed to set output." });
        return;
      }
      setF1(f1Val);
      setFreqInput("");
      setWizardStep({ type: "step2" });
    } catch (e) {
      setWizardStep({ type: "error", message: String(e) });
    }
  }, [usbDevice, freqInput, outputJack]);

  const handleStep2Calculate = useCallback(async () => {
    if (f1 === null) return;
    const f2Val = parseFloat(freqInput);
    if (!Number.isFinite(f2Val) || f2Val <= 0) return;

    releaseOutput(outputJack);

    const gain = OCTAVE_SPAN / Math.log2(f2Val / f1);
    const countsPerOct = Math.round(gain * 410);
    setWizardStep({ type: "done", countsPerOct, gain });
  }, [f1, freqInput, outputJack, releaseOutput]);

  const handleSave = useCallback(async () => {
    if (openCurveIdx === null || wizardStep.type !== "done") return;

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
                    <p className="text-sm">
                      Connect an output jack to a VCO V/Oct input, and the VCO
                      audio output to your frequency meter. Use a jack with no
                      app assigned for best accuracy.
                    </p>
                    <Select
                      label="Output jack"
                      selectedKeys={[outputJack]}
                      onSelectionChange={(keys) =>
                        setOutputJack([...keys][0] as string)
                      }
                      items={JACK_OPTIONS}
                    >
                      {(item) => (
                        <SelectItem key={item.key}>{item.label}</SelectItem>
                      )}
                    </Select>
                  </div>
                )}
                {wizardStep.type === "step1" && (
                  <div className="flex flex-col gap-y-4">
                    <p className="text-sm">
                      Outputting 1V. Read the frequency from your meter and
                      enter it below.
                    </p>
                    <Input
                      label="Measured frequency (Hz)"
                      type="number"
                      value={freqInput}
                      onValueChange={setFreqInput}
                    />
                  </div>
                )}
                {wizardStep.type === "step2" && (
                  <div className="flex flex-col gap-y-4">
                    <p className="text-sm">
                      Outputting 4V. Read the frequency from your meter and
                      enter it below.
                    </p>
                    <Input
                      label="Measured frequency (Hz)"
                      type="number"
                      value={freqInput}
                      onValueChange={setFreqInput}
                    />
                  </div>
                )}
                {wizardStep.type === "done" && (
                  <div className="flex flex-col gap-y-2">
                    <p className="text-sm font-medium">
                      Measured: {formatGain(wizardStep.countsPerOct)}
                    </p>
                    <p className="text-xs text-gray-400">
                      ({wizardStep.countsPerOct} counts/oct)
                    </p>
                  </div>
                )}
                {wizardStep.type === "error" && (
                  <p className="text-sm text-red-400">{wizardStep.message}</p>
                )}
              </ModalBody>
              <ModalFooter>
                {wizardStep.type === "setup" && (
                  <>
                    <ButtonPrimary onPress={handleStart}>Start</ButtonPrimary>
                    <ButtonSecondary onPress={onClose}>Cancel</ButtonSecondary>
                  </>
                )}
                {wizardStep.type === "step1" && (
                  <>
                    <ButtonPrimary
                      onPress={handleStep1Next}
                      isDisabled={
                        !Number.isFinite(parseFloat(freqInput)) ||
                        parseFloat(freqInput) <= 0
                      }
                    >
                      Next
                    </ButtonPrimary>
                    <ButtonSecondary onPress={onClose}>Cancel</ButtonSecondary>
                  </>
                )}
                {wizardStep.type === "step2" && (
                  <>
                    <ButtonPrimary
                      onPress={handleStep2Calculate}
                      isDisabled={
                        !Number.isFinite(parseFloat(freqInput)) ||
                        parseFloat(freqInput) <= 0
                      }
                    >
                      Calculate
                    </ButtonPrimary>
                    <ButtonSecondary onPress={onClose}>Cancel</ButtonSecondary>
                  </>
                )}
                {wizardStep.type === "done" && (
                  <>
                    <ButtonPrimary onPress={handleSave}>Save</ButtonPrimary>
                    <ButtonSecondary onPress={onClose}>Discard</ButtonSecondary>
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
