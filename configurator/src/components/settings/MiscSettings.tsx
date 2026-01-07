import { Slider } from "@heroui/slider";
import { useCallback } from "react";
import { useFormContext } from "react-hook-form";
import type { Inputs } from "../SettingsTab";

export const MiscSettings = () => {
  const { register, getValues } = useFormContext<Inputs>();

  const { onChange, onBlur, name, ref } = register("ledBrightness", {
    valueAsNumber: true,
  });

  const handleChange = useCallback(
    (value: number | number[]) => {
      if (!Array.isArray(value)) {
        onChange({ target: { name, value } });
      }
    },
    [name, onChange],
  );

  return (
    <div className="mb-12">
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
        Miscellaneous
      </h2>
      <div className="grid grid-cols-4 gap-x-16 gap-y-8 px-4">
        <Slider
          defaultValue={getValues("ledBrightness")}
          minValue={100}
          maxValue={255}
          onBlur={onBlur}
          name={name}
          onChange={handleChange}
          label="LED Brightness"
          ref={ref}
        />
      </div>
    </div>
  );
};
