import { z } from "zod";
import { Param, Value } from "@atov/fp-config";

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
