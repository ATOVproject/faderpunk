import { type UseFormRegister, type FieldValues } from "react-hook-form";
import { type Param } from "@atov/fp-config";

import { ParamI32 } from "./ParamI32.tsx";
import { ParamF32 } from "./ParamF32.tsx";
import { ParamBool } from "./ParamBool.tsx";
import { ParamNote } from "./ParamNote.tsx";
import { ParamCurve } from "./ParamCurve.tsx";
import { ParamEnum } from "./ParamEnum.tsx";
import { ParamRange } from "./ParamRange.tsx";
import { ParamWaveform } from "./ParamWaveform.tsx";
import { ParamColor } from "./ParamColor.tsx";
import { ParamMidiCc } from "./ParamMidiCc.tsx";
import { ParamMidiChannel } from "./ParamMidiChannel.tsx";
import { ParamMidiIn } from "./ParamMidiIn.tsx";
import { ParamMidiMode } from "./ParamMidiMode.tsx";
import { ParamMidiNote } from "./ParamMidiNote.tsx";
import { ParamMidiOut } from "./ParamMidiOut.tsx";

interface Props {
  defaultValue: string | boolean | number;
  param: Param;
  paramIndex: number;
  register: UseFormRegister<FieldValues>;
}

export const AppParam = ({
  defaultValue,
  param,
  paramIndex,
  register,
}: Props) => {
  switch (param.tag) {
    case "i32": {
      return (
        <ParamI32
          {...param.value}
          defaultValue={defaultValue as string}
          register={register}
          paramIndex={paramIndex}
        />
      );
    }
    case "f32": {
      return (
        <ParamF32
          {...param.value}
          defaultValue={defaultValue as string}
          paramIndex={paramIndex}
          register={register}
        />
      );
    }
    case "bool": {
      return (
        <ParamBool
          {...param.value}
          defaultValue={defaultValue as boolean}
          register={register}
          paramIndex={paramIndex}
        />
      );
    }
    case "Enum": {
      return (
        <ParamEnum
          {...param.value}
          defaultValue={defaultValue as string}
          paramIndex={paramIndex}
          register={register}
        />
      );
    }
    case "Curve": {
      return (
        <ParamCurve
          {...param.value}
          defaultValue={defaultValue as string}
          paramIndex={paramIndex}
          register={register}
        />
      );
    }
    case "Waveform": {
      return (
        <ParamWaveform
          {...param.value}
          defaultValue={defaultValue as string}
          paramIndex={paramIndex}
          register={register}
        />
      );
    }
    case "Color": {
      return (
        <ParamColor
          {...param.value}
          defaultValue={defaultValue as string}
          paramIndex={paramIndex}
          register={register}
        />
      );
    }
    case "Range": {
      return (
        <ParamRange
          {...param.value}
          defaultValue={defaultValue as string}
          paramIndex={paramIndex}
          register={register}
        />
      );
    }
    case "Note": {
      return (
        <ParamNote
          {...param.value}
          defaultValue={defaultValue as string}
          paramIndex={paramIndex}
          register={register}
        />
      );
    }
    case "MidiCc": {
      return (
        <ParamMidiCc
          {...param.value}
          defaultValue={defaultValue as string}
          paramIndex={paramIndex}
          register={register}
        />
      );
    }
    case "MidiChannel": {
      return (
        <ParamMidiChannel
          {...param.value}
          defaultValue={defaultValue as string}
          paramIndex={paramIndex}
          register={register}
        />
      );
    }
    case "MidiNote": {
      return (
        <ParamMidiNote
          {...param.value}
          defaultValue={defaultValue as string}
          paramIndex={paramIndex}
          register={register}
        />
      );
    }
    case "MidiIn": {
      return (
        <ParamMidiIn
          name="MIDI In"
          defaultValue={defaultValue as string}
          paramIndex={paramIndex}
          register={register}
        />
      );
    }
    case "MidiOut": {
      return (
        <ParamMidiOut
          name="MIDI Out"
          defaultValue={defaultValue as string}
          paramIndex={paramIndex}
          register={register}
        />
      );
    }
    case "MidiMode": {
      return (
        <ParamMidiMode
          name="MIDI Mode"
          defaultValue={defaultValue as string}
          paramIndex={paramIndex}
          register={register}
        />
      );
    }
    default: {
      return null;
    }
  }
};
