import type { AuxJackMode, ClockDivision } from "@atov/fp-config";
import { Select, SelectItem } from "@heroui/select";
import { Controller, useFormContext } from "react-hook-form";

import { Icon } from "../Icon";
import { selectProps } from "../input/defaultProps";
import type { Inputs } from "../SettingsTab";
import { useEffect } from "react";

interface AuxJackModeItem {
  key: AuxJackMode["tag"];
  value: string;
}

const auxJackModeItems: AuxJackModeItem[] = [
  { key: "None", value: "None" },
  { key: "ClockOut", value: "Clock out" },
  { key: "ResetOut", value: "Reset out" },
];

interface DivisionItem {
  key: ClockDivision["tag"];
  value: string;
}

const auxDivisionItems: DivisionItem[] = [
  { key: "_1", value: "24 PPQN" },
  { key: "_2", value: "12 PPQN" },
  { key: "_4", value: "6 PPQN" },
  { key: "_6", value: "4 PPQN" },
  { key: "_8", value: "3 PPQN" },
  { key: "_12", value: "2 PPQN" },
  { key: "_24", value: "1 PPQN" },
  { key: "_96", value: "1 Bar" },
  { key: "_192", value: "2 Bars" },
  { key: "_384", value: "4 Bars" },
];

export const AuxSettings = () => {
  const { control, register, setValue, watch } = useFormContext<Inputs>();

  const [clockSrc, resetSrc] = watch(["clockSrc", "resetSrc"]);

  const [atomMode, meteorMode, cubeMode] = watch([
    "auxAtom",
    "auxMeteor",
    "auxCube",
  ]);

  useEffect(() => {
    if (clockSrc === "Atom" || resetSrc === "Atom") {
      setValue("auxAtom", "None");
    }
    if (clockSrc === "Meteor" || resetSrc === "Meteor") {
      setValue("auxMeteor", "None");
    }
    if (clockSrc === "Cube" || resetSrc === "Cube") {
      setValue("auxCube", "None");
    }
  }, [clockSrc, resetSrc, setValue]);

  return (
    <div className="mb-12">
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
        Aux Jacks
      </h2>
      <div className="grid grid-cols-4 gap-x-16 gap-y-8 px-4">
        <div className="flex flex-col gap-y-4">
          <Controller
            name="auxAtom"
            control={control}
            render={({ field }) => (
              <Select
                selectedKeys={[field.value]}
                onSelectionChange={(value) => {
                  field.onChange(value.currentKey);
                }}
                isDisabled={clockSrc === "Atom" || resetSrc === "Atom"}
                {...selectProps}
                label={
                  <div className="flex items-center">
                    <Icon className="text-cyan-fp h-4 w-4" name="atom" />
                    Atom Mode
                  </div>
                }
                items={auxJackModeItems}
                placeholder="Atom Mode"
              >
                {(item) => <SelectItem>{item.value}</SelectItem>}
              </Select>
            )}
          ></Controller>
          {atomMode == "ClockOut" && (
            <Select
              {...register("auxAtomDiv")}
              {...selectProps}
              label="Division"
              items={auxDivisionItems}
              placeholder="Division"
            >
              {(item) => <SelectItem>{item.value}</SelectItem>}
            </Select>
          )}
        </div>
        <div className="flex flex-col gap-y-4">
          <Controller
            name="auxMeteor"
            control={control}
            render={({ field }) => (
              <Select
                selectedKeys={[field.value]}
                onSelectionChange={(value) => {
                  field.onChange(value.currentKey);
                }}
                isDisabled={clockSrc === "Meteor" || resetSrc === "Meteor"}
                {...selectProps}
                label={
                  <div className="flex items-center">
                    <Icon className="text-yellow-fp h-4 w-4" name="meteor" />
                    Meteor Mode
                  </div>
                }
                items={auxJackModeItems}
                placeholder="Meteor Mode"
              >
                {(item) => <SelectItem>{item.value}</SelectItem>}
              </Select>
            )}
          ></Controller>
          {meteorMode == "ClockOut" && (
            <Select
              {...register("auxMeteorDiv")}
              {...selectProps}
              label="Division"
              items={auxDivisionItems}
              placeholder="Division"
            >
              {(item) => <SelectItem>{item.value}</SelectItem>}
            </Select>
          )}
        </div>
        <div className="flex flex-col gap-y-4">
          <Controller
            name="auxCube"
            control={control}
            render={({ field }) => (
              <Select
                selectedKeys={[field.value]}
                onSelectionChange={(value) => {
                  field.onChange(value.currentKey);
                }}
                isDisabled={clockSrc === "Cube" || resetSrc === "Cube"}
                {...selectProps}
                label={
                  <div className="flex items-center">
                    <Icon className="text-pink-fp h-4 w-4" name="cube" />
                    Cube Mode
                  </div>
                }
                items={auxJackModeItems}
                placeholder="Cube Mode"
              >
                {(item) => <SelectItem>{item.value}</SelectItem>}
              </Select>
            )}
          ></Controller>
          {cubeMode == "ClockOut" && (
            <Select
              {...register("auxCubeDiv")}
              {...selectProps}
              label="Division"
              items={auxDivisionItems}
              placeholder="Division"
            >
              {(item) => <SelectItem>{item.value}</SelectItem>}
            </Select>
          )}
        </div>
      </div>
    </div>
  );
};
