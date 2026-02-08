import type {
  AuxJackMode,
  ClockDivision,
  ClockSrc,
  FixedLengthArray,
  GlobalConfig,
  I2cMode,
  Key,
  MidiOutConfig,
  MidiOutMode,
  Note,
  ResetSrc,
} from "@atov/fp-config";
import { FormProvider, useForm, type SubmitHandler } from "react-hook-form";
import { useCallback, useEffect, useState } from "react";

import { ClockSettings } from "./settings/ClockSettings";
import { ButtonPrimary } from "./Button";
import { Icon } from "./Icon";
import { useStore } from "../store";
import { I2cSettings } from "./settings/I2cSettings";
import { QuantizerSettings } from "./settings/QuantizerSettings";
import { AuxSettings } from "./settings/AuxSettings";
import { MidiSettings } from "./settings/MidiSettings";
import { MiscSettings } from "./settings/MiscSettings";
import { setGlobalConfig } from "../utils/config";
import { FactoryReset } from "./settings/FactoryReset";
import { SaveLoadSetup } from "./SaveLoadSetup";

interface SettingsFormProps {
  config: GlobalConfig;
}

export interface Inputs {
  auxAtom: AuxJackMode["tag"];
  auxMeteor: AuxJackMode["tag"];
  auxCube: AuxJackMode["tag"];
  auxAtomDiv: ClockDivision["tag"];
  auxMeteorDiv: ClockDivision["tag"];
  auxCubeDiv: ClockDivision["tag"];
  clockSrc: ClockSrc["tag"];
  i2cMode: I2cMode["tag"];
  internalBpm: number;
  ledBrightness: number;
  resetSrc: ResetSrc["tag"];
  quantizerKey: Key["tag"];
  quantizerTonic: Note["tag"];
  // MIDI USB
  midiUsbMode: MidiOutMode["tag"];
  midiUsbSendClock: boolean;
  midiUsbSendTransport: boolean;
  midiUsbSourceUsb: boolean;
  midiUsbSourceDin: boolean;
  // MIDI Out 1
  midiOut1Mode: MidiOutMode["tag"];
  midiOut1SendClock: boolean;
  midiOut1SendTransport: boolean;
  midiOut1SourceUsb: boolean;
  midiOut1SourceDin: boolean;
  // MIDI Out 2
  midiOut2Mode: MidiOutMode["tag"];
  midiOut2SendClock: boolean;
  midiOut2SendTransport: boolean;
  midiOut2SourceUsb: boolean;
  midiOut2SourceDin: boolean;
}

const SettingsForm = ({ config }: SettingsFormProps) => {
  const { usbDevice, deviceVersion, setConfig } = useStore();

  // Helper to extract MIDI output config values
  const getMidiOutValues = (index: number) => {
    const out = config.midi.outs[index];
    const mode = out.mode.tag;

    if (mode === "MidiThru" || mode === "MidiMerge") {
      return {
        mode: mode,
        sendClock: out.send_clock,
        sendTransport: out.send_transport,
        sourceUsb: out.mode.value.sources[0][0],
        sourceDin: out.mode.value.sources[0][1],
      };
    }

    return {
      mode: mode as "None" | "Local",
      sendClock: out.send_clock,
      sendTransport: out.send_transport,
      sourceUsb: false,
      sourceDin: false,
    };
  };

  const midiUsb = getMidiOutValues(0);
  const midiOut1 = getMidiOutValues(1);
  const midiOut2 = getMidiOutValues(2);

  const methods = useForm<Inputs>({
    defaultValues: {
      auxAtom: config.aux[0].tag,
      auxMeteor: config.aux[1].tag,
      auxCube: config.aux[2].tag,
      auxAtomDiv:
        "value" in config.aux[0] ? config.aux[0].value.tag : undefined,
      auxMeteorDiv:
        "value" in config.aux[1] ? config.aux[1].value.tag : undefined,
      auxCubeDiv:
        "value" in config.aux[2] ? config.aux[2].value.tag : undefined,
      clockSrc: config.clock.clock_src.tag,
      resetSrc: config.clock.reset_src.tag,
      internalBpm: config.clock.internal_bpm,
      i2cMode: config.i2c_mode.tag,
      quantizerKey: config.quantizer.key.tag,
      quantizerTonic: config.quantizer.tonic.tag,
      ledBrightness: config.led_brightness,
      // MIDI USB
      midiUsbMode: midiUsb.mode,
      midiUsbSendClock: midiUsb.sendClock,
      midiUsbSendTransport: midiUsb.sendTransport,
      midiUsbSourceUsb: midiUsb.sourceUsb,
      midiUsbSourceDin: midiUsb.sourceDin,
      // MIDI Out 1
      midiOut1Mode: midiOut1.mode,
      midiOut1SendClock: midiOut1.sendClock,
      midiOut1SendTransport: midiOut1.sendTransport,
      midiOut1SourceUsb: midiOut1.sourceUsb,
      midiOut1SourceDin: midiOut1.sourceDin,
      // MIDI Out 2
      midiOut2Mode: midiOut2.mode,
      midiOut2SendClock: midiOut2.sendClock,
      midiOut2SendTransport: midiOut2.sendTransport,
      midiOut2SourceUsb: midiOut2.sourceUsb,
      midiOut2SourceDin: midiOut2.sourceDin,
    },
  });
  const [saved, setSaved] = useState<boolean>(false);
  const [configuratorVersion, setConfiguratorVersion] = useState<string>("");
  const {
    handleSubmit,
    formState: { isSubmitting },
  } = methods;

  const onSubmit: SubmitHandler<Inputs> = useCallback(
    async (formValues: Inputs) => {
      if (usbDevice) {
        const config = transformFormToGlobalConfig(formValues);
        await setGlobalConfig(usbDevice, config);
        setConfig(config);
        setSaved(true);
        setTimeout(() => {
          setSaved(false);
        }, 2000);
      }
    },
    [usbDevice, setConfig],
  );

  useEffect(() => {
    const getVersion = async () => {
      const packageJson = await import("../../package.json");
      setConfiguratorVersion(packageJson.version);
    };
    getVersion();
  }, []);

  return (
    <FormProvider {...methods}>
      <form onSubmit={handleSubmit(onSubmit)}>
        <ClockSettings />
        <AuxSettings />
        <QuantizerSettings />
        <MidiSettings />
        <I2cSettings />
        <MiscSettings />
        <SaveLoadSetup />
        <FactoryReset />
        <div className="flex justify-between">
          <p>
            Your current version: Faderpunk v{deviceVersion}, Configurator v
            {configuratorVersion}
          </p>
          <ButtonPrimary
            color={saved ? "success" : "primary"}
            isDisabled={isSubmitting}
            isLoading={isSubmitting}
            startContent={
              saved ? <Icon className="h-5 w-5" name="check" /> : undefined
            }
            type="submit"
          >
            {saved ? "Saved" : "Save"}
          </ButtonPrimary>
        </div>
      </form>
    </FormProvider>
  );
};

interface Props {
  config?: GlobalConfig;
}

export const SettingsTab = ({ config }: Props) => {
  // TODO: loading skeleton
  if (!config) {
    return null;
  }

  return <SettingsForm config={config} />;
};

const buildAuxJackMode = (
  modeTag: AuxJackMode["tag"],
  divTag?: ClockDivision["tag"],
): AuxJackMode => {
  if (modeTag === "ClockOut") {
    // Default to _24 if for some reason a division isn't provided for ClockOut mode
    return { tag: "ClockOut", value: { tag: divTag ?? "_24" } };
  }
  return { tag: modeTag as "None" | "ResetOut" };
};

const buildMidiOutConfig = (
  mode: MidiOutMode["tag"],
  sendClock: boolean,
  sendTransport: boolean,
  sourceUsb: boolean,
  sourceDin: boolean,
): MidiOutConfig => {
  if (mode === "MidiThru" || mode === "MidiMerge") {
    // MidiIn is a tuple struct: [FixedLengthArray<boolean, 2>]
    const sources = [
      [sourceUsb, sourceDin] as FixedLengthArray<boolean, 2>,
    ] as [FixedLengthArray<boolean, 2>];

    return {
      send_clock: sendClock,
      send_transport: sendTransport,
      mode: {
        tag: mode,
        value: {
          sources,
        },
      },
    };
  }

  return {
    send_clock: sendClock,
    send_transport: sendTransport,
    mode: { tag: mode },
  };
};

const transformFormToGlobalConfig = (formValues: Inputs): GlobalConfig => {
  const auxArray = [
    buildAuxJackMode(formValues.auxAtom, formValues.auxAtomDiv),
    buildAuxJackMode(formValues.auxMeteor, formValues.auxMeteorDiv),
    buildAuxJackMode(formValues.auxCube, formValues.auxCubeDiv),
  ] as FixedLengthArray<AuxJackMode, 3>;

  const midiOutsArray = [
    buildMidiOutConfig(
      formValues.midiUsbMode,
      formValues.midiUsbSendClock,
      formValues.midiUsbSendTransport,
      false, // USB output cannot route from USB input
      true, // USB output always routes from DIN (only valid source)
    ),
    buildMidiOutConfig(
      formValues.midiOut1Mode,
      formValues.midiOut1SendClock,
      formValues.midiOut1SendTransport,
      formValues.midiOut1SourceUsb,
      formValues.midiOut1SourceDin,
    ),
    buildMidiOutConfig(
      formValues.midiOut2Mode,
      formValues.midiOut2SendClock,
      formValues.midiOut2SendTransport,
      formValues.midiOut2SourceUsb,
      formValues.midiOut2SourceDin,
    ),
  ] as FixedLengthArray<MidiOutConfig, 3>;

  return {
    aux: auxArray,
    clock: {
      clock_src: { tag: formValues.clockSrc },
      ext_ppqn: 24,
      reset_src: { tag: formValues.resetSrc },
      internal_bpm: formValues.internalBpm,
    },
    i2c_mode: { tag: formValues.i2cMode },
    led_brightness: formValues.ledBrightness,
    midi: {
      outs: midiOutsArray,
    },
    quantizer: {
      key: { tag: formValues.quantizerKey },
      tonic: { tag: formValues.quantizerTonic },
    },
  };
};
