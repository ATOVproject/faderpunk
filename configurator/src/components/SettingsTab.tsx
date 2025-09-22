import type {
  AuxJackMode,
  ClockDivision,
  ClockSrc,
  GlobalConfig,
  I2cMode,
  Key,
  Note,
  ResetSrc,
} from "@atov/fp-config";
import { FormProvider, useForm, type SubmitHandler } from "react-hook-form";
import { useCallback, useState } from "react";

import { ClockSettings } from "./settings/ClockSettings";
import { ButtonPrimary } from "./Button";
import { Icon } from "./Icon";
import { useStore } from "../store";
import { I2cSettings } from "./settings/I2cSettings";
import { QuantizerSettings } from "./settings/QuantizerSettings";
import { AuxSettings } from "./settings/AuxSettings";
import { MiscSettings } from "./settings/MiscSettings";
import { setGlobalConfig } from "../utils/config";

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
}

const SettingsForm = ({ config }: SettingsFormProps) => {
  const { usbDevice } = useStore();
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
    },
  });
  const [saved, setSaved] = useState<boolean>(false);
  const {
    handleSubmit,
    formState: { isSubmitting },
  } = methods;

  const onSubmit: SubmitHandler<Inputs> = useCallback(
    async (formValues: Inputs) => {
      if (usbDevice) {
        const config = transformFormToGlobalConfig(formValues);
        await setGlobalConfig(usbDevice, config);
        setSaved(true);
        setTimeout(() => {
          setSaved(false);
        }, 2000);
      }
    },
    [usbDevice],
  );

  return (
    <FormProvider {...methods}>
      <form onSubmit={handleSubmit(onSubmit)}>
        <ClockSettings />
        <QuantizerSettings />
        <I2cSettings />
        <AuxSettings />
        <MiscSettings />
        <div className="flex justify-end">
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

const transformFormToGlobalConfig = (formValues: Inputs): GlobalConfig => {
  return {
    aux: [
      buildAuxJackMode(formValues.auxAtom, formValues.auxAtomDiv),
      buildAuxJackMode(formValues.auxMeteor, formValues.auxMeteorDiv),
      buildAuxJackMode(formValues.auxCube, formValues.auxCubeDiv),
    ],
    clock: {
      clock_src: { tag: formValues.clockSrc },
      reset_src: { tag: formValues.resetSrc },
      internal_bpm: formValues.internalBpm,
      ext_ppqn: 24,
    },
    i2c_mode: { tag: formValues.i2cMode },
    quantizer: {
      key: { tag: formValues.quantizerKey },
      tonic: { tag: formValues.quantizerTonic },
    },
    led_brightness: formValues.ledBrightness,
  };
};
