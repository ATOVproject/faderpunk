import { z } from "zod";
import { GlobalConfig, Param, Value } from "@atov/fp-config";

export const getParamSchema = (param: Param) => {
  switch (param.tag) {
    case "i32": {
      const { min, max } = param.value;
      return z
        .object({
          tag: z.literal("i32"),
          value: z.number().int().min(min).max(max),
        })
        .default({ tag: "i32", value: 0 });
    }
    case "f32": {
      const { min, max } = param.value;
      return z
        .object({
          tag: z.literal("f32"),
          value: z.number().min(min).max(max),
        })
        .default({ tag: "f32", value: 0.0 });
    }
    case "bool": {
      return z
        .object({
          tag: z.literal("bool"),
          value: z.boolean(),
        })
        .default({ tag: "bool", value: false });
    }
    case "Enum": {
      const choices = param.value.variants.map((_val, idx) => idx);
      if (choices.length === 0) {
        // This case should ideally not happen with valid params
        return z.never();
      }
      return z
        .object({
          tag: z.literal("Enum"),
          value: z.number().int().transform(BigInt),
        })
        .refine((val) => choices.includes(Number(val.value)), {
          message: "Invalid enum value",
        })
        .catch({ tag: "Enum", value: BigInt(choices[0]) });
    }
    case "Curve":
    case "Waveform":
    case "Range":
    case "Note": {
      const choices = param.value.variants.map((v) => v.tag);
      if (choices.length === 0) return z.never();
      const enumSchema = z.enum(choices as [string, ...string[]]);
      return z
        .object({
          tag: z.literal(param.tag),
          value: z.object({ tag: enumSchema }),
        })
        .catch({
          tag: param.tag,
          value: { tag: choices[0] },
        });
    }
    case "Color": {
      const choices = param.value.variants.map((v) => v.tag);
      if (choices.length === 0) return z.never();
      const enumSchema = z.enum(choices as [string, ...string[]]);
      return z
        .object({
          tag: z.literal(param.tag),
          value: z.object({ tag: enumSchema }),
        })
        .catch({
          tag: param.tag,
          value: { tag: choices[0] },
        });
    }
    case "MidiCc": {
      return z
        .object({
          tag: z.literal("MidiCc"),
          value: z.tuple([z.number().int().min(0).max(127)]),
        })
        .default({ tag: "MidiCc", value: [0] });
    }
    case "MidiChannel": {
      return z
        .object({
          tag: z.literal("MidiChannel"),
          value: z.tuple([z.number().int().min(0).max(15)]),
        })
        .default({ tag: "MidiChannel", value: [0] });
    }
    case "MidiNote": {
      return z
        .object({
          tag: z.literal("MidiNote"),
          value: z.tuple([z.number().int().min(0).max(127)]),
        })
        .default({ tag: "MidiNote", value: [60] });
    }
    case "MidiIn": {
      // MidiIn is a tuple struct: [[usb, din]]
      return z
        .object({
          tag: z.literal("MidiIn"),
          value: z.tuple([z.tuple([z.boolean(), z.boolean()])]),
        })
        .default({ tag: "MidiIn", value: [[false, false]] });
    }
    case "MidiOut": {
      // MidiOut is a tuple struct: [[usb, out1, out2]]
      return z
        .object({
          tag: z.literal("MidiOut"),
          value: z.tuple([z.tuple([z.boolean(), z.boolean(), z.boolean()])]),
        })
        .default({ tag: "MidiOut", value: [[false, false, false]] });
    }
    case "MidiMode": {
      // MidiMode is still enum-like with tag-based variants
      return z
        .object({
          tag: z.literal("MidiMode"),
          value: z.object({ tag: z.string() }),
        })
        .catch({
          tag: "MidiMode",
          value: { tag: "Note" },
        });
    }
    default: {
      return z.never();
    }
  }
};

export const parseParamValueFromFile = (
  param: Param,
  fileValue: Value | undefined,
): Value => {
  if (param.tag === "None") {
    throw new Error("Empty params are not allowed");
  }

  const schema = getParamSchema(param);
  const result = schema.safeParse(fileValue);

  if (result.success) {
    return result.data as Value;
  }

  // If parsing fails, return the schema's default value
  return schema.parse(undefined) as Value;
};

// Default GlobalConfig matching libfp defaults
const defaultGlobalConfig: GlobalConfig = {
  aux: [
    { tag: "ClockOut", value: { tag: "_1" } },
    { tag: "None" },
    { tag: "None" },
  ],
  clock: {
    clock_src: { tag: "Internal" },
    ext_ppqn: 24,
    reset_src: { tag: "None" },
    internal_bpm: 120.0,
  },
  i2c_mode: { tag: "Leader" },
  led_brightness: 150,
  midi: {
    outs: [
      {
        send_clock: true,
        send_transport: true,
        mode: { tag: "Local" },
      },
      {
        send_clock: true,
        send_transport: true,
        mode: { tag: "Local" },
      },
      {
        send_clock: true,
        send_transport: true,
        mode: { tag: "Local" },
      },
    ],
  },
  quantizer: {
    key: { tag: "Chromatic" },
    tonic: { tag: "C" },
  },
  takeover_mode: { tag: "Pickup" },
};

// Lenient schema that validates structure but allows any valid tag values
const taggedObjectSchema = z.object({ tag: z.string() }).passthrough();

const globalConfigSchema = z.object({
  aux: z.array(taggedObjectSchema).length(3),
  clock: z.object({
    clock_src: taggedObjectSchema,
    ext_ppqn: z.number().int().min(1).max(96),
    reset_src: taggedObjectSchema,
    internal_bpm: z.number().min(1).max(300),
  }),
  i2c_mode: taggedObjectSchema,
  led_brightness: z.number().int().min(100).max(255),
  midi: z.object({
    outs: z
      .array(
        z.object({
          send_clock: z.boolean(),
          send_transport: z.boolean(),
          mode: taggedObjectSchema,
        }),
      )
      .length(3),
  }),
  quantizer: z.object({
    key: taggedObjectSchema,
    tonic: taggedObjectSchema,
  }),
  takeover_mode: taggedObjectSchema,
});

export const parseGlobalConfigFromFile = (
  fileConfig: unknown,
): GlobalConfig => {
  const result = globalConfigSchema.safeParse(fileConfig);

  if (!result.success) {
    // If parsing fails, return the default config
    return defaultGlobalConfig;
  }

  // Manually construct a properly-typed GlobalConfig from validated data
  const validated = result.data;

  const config: GlobalConfig = {
    aux: [
      validated.aux[0],
      validated.aux[1],
      validated.aux[2],
    ] as GlobalConfig["aux"],
    clock: validated.clock as GlobalConfig["clock"],
    i2c_mode: validated.i2c_mode as GlobalConfig["i2c_mode"],
    led_brightness: validated.led_brightness,
    midi: {
      outs: [
        validated.midi.outs[0],
        validated.midi.outs[1],
        validated.midi.outs[2],
      ] as GlobalConfig["midi"]["outs"],
    },
    quantizer: validated.quantizer as GlobalConfig["quantizer"],
    takeover_mode: validated.takeover_mode as GlobalConfig["takeover_mode"],
  };

  return config;
};
