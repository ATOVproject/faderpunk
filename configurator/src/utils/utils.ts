import {
  type Color,
  type Curve,
  type FixedLengthArray,
  type Note,
  type Range,
  type Value,
  type Waveform,
} from "@atov/fp-config";

import type { AllApps, App, AppLayout } from "./types";
import type { MidiInTag, MidiOutTag, MidiModeTag } from "./midiTypes";

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

export const getSlots = (app: App, startChannel: number) => {
  if (app.channels > 1) {
    return `${startChannel + 1}-${startChannel + Number(app.channels)}`;
  } else {
    return `${startChannel + 1}`;
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
      return Number(val.value);
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
    case "MidiCc": {
      return val.value[0].toString();
    }
    case "MidiChannel": {
      return val.value[0].toString();
    }
    case "MidiNote": {
      return val.value[0].toString();
    }
    case "MidiIn": {
      return val.value.tag;
    }
    case "MidiOut": {
      return val.value.tag;
    }
    case "MidiMode": {
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
    case "MidiCc":
      return { tag: "MidiCc", value: [parseInt(value as string, 10)] };
    case "MidiChannel":
      return { tag: "MidiChannel", value: [parseInt(value as string, 10)] };
    case "MidiNote":
      return { tag: "MidiNote", value: [parseInt(value as string, 10)] };
    case "MidiIn":
      return {
        tag: "MidiIn",
        value: { tag: value as MidiInTag },
      };
    case "MidiOut":
      return {
        tag: "MidiOut",
        value: { tag: value as MidiOutTag },
      };
    case "MidiMode":
      return { tag: "MidiMode", value: { tag: value as MidiModeTag } };
    default:
      return undefined;
  }
};

export const transformParamFormValues = (
  values: Record<string, string | boolean>,
) => {
  const entries = Object.entries(values);
  const result: FixedLengthArray<Value | undefined, 16> = [
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
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

export const getFixedLengthParamArray = (values: Value[]) => {
  const result: FixedLengthArray<Value | undefined, 16> = [
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
  ];

  values.forEach((value, idx) => {
    result[idx] = value;
  });

  return result;
};

export const groupAndSortApps = (allApps: AllApps): App[][] => {
  return Array.from(
    Array.from(allApps.values())
      .reduce((groups, app) => {
        const existing = groups.get(app.channels) || [];
        existing.push(app);
        groups.set(app.channels, existing);
        return groups;
      }, new Map<bigint, App[]>())
      .entries(),
  )
    .sort(([channelsA], [channelsB]) => {
      // Sort by channels (ascending)
      if (channelsA < channelsB) return -1;
      if (channelsA > channelsB) return 1;
      return 0;
    })
    .map(([, apps]) => apps.sort((a, b) => a.name.localeCompare(b.name)));
};

export const recalculateStartChannels = (layout: AppLayout) => {
  let runningChannel = 0;
  return layout.map((item) => {
    const newItem = { ...item, startChannel: runningChannel };
    runningChannel += Number(item.app?.channels) || 1;
    return newItem;
  });
};

const findFreeSlot = (layout: AppLayout, requiredChannels: number) => {
  if (requiredChannels <= 0) {
    return null;
  }

  const emptySlots = layout
    .filter((slot) => slot.app === null)
    .sort((a, b) => a.startChannel - b.startChannel);

  if (emptySlots.length < requiredChannels) {
    return null;
  }

  for (let i = 0; i <= emptySlots.length - requiredChannels; i++) {
    let isContiguous = true;
    for (let j = 0; j < requiredChannels - 1; j++) {
      if (
        emptySlots[i + j].startChannel + 1 !==
        emptySlots[i + j + 1].startChannel
      ) {
        isContiguous = false;
        break;
      }
    }

    if (isContiguous) {
      return emptySlots[i].startChannel;
    }
  }

  return null;
};

export const addAppToLayout = (layout: AppLayout, appToAdd: App) => {
  const requiredChannels = Number(appToAdd.channels);
  const startChannel = findFreeSlot(layout, requiredChannels);

  if (startChannel === null) {
    return { success: false, newLayout: layout, newId: null };
  }

  const usedIds = new Set(
    layout.filter((item) => item.app).map((item) => item.id),
  );

  let newId = -1;
  for (let i = 0; i < 16; i++) {
    if (!usedIds.has(i)) {
      newId = i;
      break;
    }
  }

  if (newId === -1) {
    return { success: false, newLayout: layout, newId: null };
  }

  const slotsToReplace = layout.filter(
    (item) =>
      !item.app &&
      item.startChannel >= startChannel &&
      item.startChannel < startChannel + requiredChannels,
  );

  const newAppItem = {
    id: newId,
    app: appToAdd,
    startChannel: startChannel,
  };

  const slotsToReplaceIds = new Set(slotsToReplace.map((s) => s.id));
  const layoutWithoutReplacedSlots = layout.filter(
    (item) => !slotsToReplaceIds.has(item.id),
  );

  const newLayoutWithApp = [...layoutWithoutReplacedSlots, newAppItem].sort(
    (a, b) => a.startChannel - b.startChannel,
  );

  return {
    success: true,
    newLayout: recalculateStartChannels(newLayoutWithApp),
    newId,
  };
};

export const delay = (ms: number): Promise<void> => {
  return new Promise((resolve) => setTimeout(resolve, ms));
};
