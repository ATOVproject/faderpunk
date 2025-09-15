import {
  type Color,
  type Curve,
  type FixedLengthArray,
  type Note,
  type Range,
  type Value,
  type Waveform,
} from "@atov/fp-config";

import type { AppInLayout } from "./types";

const defaultInitializer = (index: number) => index;

export const createRange = <T = number>(
  length: number,
  initializer: (index: number) => any = defaultInitializer,
): T[] => {
  return [...new Array(length)].map((_, index) => initializer(index));
};

export const kebabToPascal = (str: string): string => {
  if (!str) return "";
  return str
    .split("-")
    .map((word) => (word ? word.charAt(0).toUpperCase() + word.slice(1) : ""))
    .join("");
};

export const pascalToKebab = (str: string): string => {
  if (!str) return "";
  const camelized = str.replace(/^./, (c) => c.toLowerCase());
  return camelized.replace(/([A-Z])/g, "-$1").toLowerCase();
};

export const getSlots = (app: AppInLayout) => {
  if (app.channels > 1) {
    return `${app.start + 1}-${app.end + 1}`;
  } else {
    return `${app.start + 1}`;
  }
};

export const getDefaultValue = (val: Value) => {
  switch (val.tag) {
    case "i32": {
      return val.value.toString();
    }
    case "f32": {
      return val.value.toString();
    }
    case "Enum": {
      return val.value.toString();
    }
    case "bool": {
      return val.value;
    }
    case "Curve": {
      return val.value.tag;
    }
    case "Waveform": {
      return val.value.tag;
    }
    case "Color": {
      return val.value.tag;
    }
    case "Range": {
      return val.value.tag;
    }
    case "Note": {
      return val.value.tag;
    }
  }
};

const getParamValue = (
  paramType: Value["tag"],
  value: string | boolean,
): Value | undefined => {
  switch (paramType) {
    case "i32":
      return { tag: "i32", value: parseInt(value as string, 10) };
    case "f32":
      return { tag: "f32", value: parseInt(value as string, 10) };
    case "bool":
      return { tag: "bool", value: value as boolean };
    case "Enum":
      return { tag: "Enum", value: BigInt(value as string) };
    case "Curve":
      return { tag: "Curve", value: { tag: value as Curve["tag"] } };
    case "Waveform":
      return { tag: "Waveform", value: { tag: value as Waveform["tag"] } };
    case "Color":
      return {
        tag: "Color",
        value:
          value === "Custom"
            ? { tag: "Custom", value: [0, 0, 0] }
            : { tag: value as Exclude<Color["tag"], "Custom"> },
      };
    case "Range":
      return { tag: "Range", value: { tag: value as Range["tag"] } };
    case "Note":
      return { tag: "Note", value: { tag: value as Note["tag"] } };
    default:
      return undefined;
  }
};

export const transformParamValues = (
  values: Record<string, string | boolean>,
) => {
  const entries = Object.entries(values);
  const result: FixedLengthArray<Value | undefined, 8> = [
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
  ];

  entries.forEach(([key, value]) => {
    const [, paramType, pIndex] = key.split("-");
    const paramIndex = parseInt(pIndex, 10);
    const paramValue = getParamValue(paramType as Value["tag"], value);
    result[paramIndex] = paramValue;
  });

  return result;
};
