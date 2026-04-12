import type { AuxJackMode, ClockDivision } from "@atov/fp-config";
import { SelectItem } from "@heroui/select";
import { useFormContext } from "react-hook-form";

import { Icon } from "../Icon";
import type { Inputs } from "../SettingsTab";
import { useEffect } from "react";
import { ControlledSelect } from "./ControlledFields";

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
  const { control, setValue, watch } = useFormContext<Inputs>();

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
          <ControlledSelect
            name="auxAtom"
            control={control}
            items={auxJackModeItems}
            isDisabled={clockSrc === "Atom" || resetSrc === "Atom"}
            label={
              <div className="flex items-center">
                <Icon className="text-cyan-fp h-4 w-4" name="atom" />
                Atom Mode
              </div>
            }
            placeholder="Atom Mode"
          >
            {(item) => <SelectItem>{item.value}</SelectItem>}
          </ControlledSelect>
          {atomMode == "ClockOut" && (
            <ControlledSelect
              name="auxAtomDiv"
              control={control}
              items={auxDivisionItems}
              label="Division"
              placeholder="Division"
            >
              {(item) => <SelectItem>{item.value}</SelectItem>}
            </ControlledSelect>
          )}
        </div>
        <div className="flex flex-col gap-y-4">
          <ControlledSelect
            name="auxMeteor"
            control={control}
            items={auxJackModeItems}
            isDisabled={clockSrc === "Meteor" || resetSrc === "Meteor"}
            label={
              <div className="flex items-center">
                <Icon className="text-yellow-fp h-4 w-4" name="meteor" />
                Meteor Mode
              </div>
            }
            placeholder="Meteor Mode"
          >
            {(item) => <SelectItem>{item.value}</SelectItem>}
          </ControlledSelect>
          {meteorMode == "ClockOut" && (
            <ControlledSelect
              name="auxMeteorDiv"
              control={control}
              items={auxDivisionItems}
              label="Division"
              placeholder="Division"
            >
              {(item) => <SelectItem>{item.value}</SelectItem>}
            </ControlledSelect>
          )}
        </div>
        <div className="flex flex-col gap-y-4">
          <ControlledSelect
            name="auxCube"
            control={control}
            items={auxJackModeItems}
            isDisabled={clockSrc === "Cube" || resetSrc === "Cube"}
            label={
              <div className="flex items-center">
                <Icon className="text-pink-fp h-4 w-4" name="cube" />
                Cube Mode
              </div>
            }
            placeholder="Cube Mode"
          >
            {(item) => <SelectItem>{item.value}</SelectItem>}
          </ControlledSelect>
          {cubeMode == "ClockOut" && (
            <ControlledSelect
              name="auxCubeDiv"
              control={control}
              items={auxDivisionItems}
              label="Division"
              placeholder="Division"
            >
              {(item) => <SelectItem>{item.value}</SelectItem>}
            </ControlledSelect>
          )}
        </div>
      </div>
    </div>
  );
};
