import { useCallback, useState } from "react";
import { type Param, type Value } from "@atov/fp-config";
import { Skeleton } from "@heroui/skeleton";
import {
  useForm,
  type UseFormRegister,
  type FieldValues,
} from "react-hook-form";
import classNames from "classnames";

import { COLORS_CLASSES } from "../utils/class-helpers";
import { pascalToKebab, getDefaultValue, getSlots } from "../utils/utils";
import { ButtonPrimary } from "./Button";
import { Icon } from "./Icon";
import type { AppInLayout } from "../utils/types";
import { ParamI32 } from "./input/ParamI32.tsx";
import { ParamFloat } from "./input/ParamFloat.tsx";
import { ParamBool } from "./input/ParamBool.tsx";
import { ParamNote } from "./input/ParamNote.tsx";
import { ParamCurve } from "./input/ParamCurve.tsx";
import { ParamEnum } from "./input/ParamEnum.tsx";
import { ParamRange } from "./input/ParamRange.tsx";
import { ParamWaveform } from "./input/ParamWaveform.tsx";
import { ParamColor } from "./input/ParamColor.tsx";
import { getAppParams, setAppParams } from "../utils/config.ts";
import { useStore } from "../store.ts";

const ParamSkeleton = () => (
  <div className="w-40">
    <Skeleton className="mb-2 rounded-xs">
      <div className="h-5" />
    </Skeleton>
    <Skeleton className="rounded-xs">
      <div className="h-10" />
    </Skeleton>
  </div>
);

interface AppParamProps {
  defaultValue: string | boolean;
  param: Param;
  paramIndex: number;
  register: UseFormRegister<FieldValues>;
}

const AppParam = ({
  defaultValue,
  param,
  paramIndex,
  register,
}: AppParamProps) => {
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
    case "Float": {
      return (
        <ParamFloat
          {...param.value}
          defaultValue={defaultValue as string}
          paramIndex={paramIndex}
          register={register}
        />
      );
    }
    case "Bool": {
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
    default: {
      return null;
    }
  }
};

interface Props {
  app: AppInLayout;
}

// TODO: Save button turns green after save (and says "Saved") for a couple of seconds

export const ActiveApp = ({ app }: Props) => {
  const { usbDevice } = useStore();
  const [hasBeenOpened, setHasBeenOpened] = useState<boolean>(false);
  const [currentParamValues, setParams] = useState<Value[]>();
  const {
    register,
    handleSubmit,
    formState: { isSubmitting },
  } = useForm();

  const handleToggle = useCallback(
    async (e: React.SyntheticEvent<HTMLDetailsElement>) => {
      if (e.currentTarget.open && !hasBeenOpened) {
        setHasBeenOpened(true);
        if (usbDevice) {
          const params = await getAppParams(usbDevice, app.start);
          setParams(params);
        }
      }
    },
    [hasBeenOpened, usbDevice, app.start],
  );

  const onSubmit = async (data: Record<string, string | boolean>) => {
    if (usbDevice) {
      return setAppParams(usbDevice, app.start, data);
    }
  };

  return (
    <form onSubmit={handleSubmit(onSubmit)}>
      <details className="group w-full bg-black" onToggle={handleToggle}>
        <summary
          className={classNames(
            "flex list-none items-center gap-4 p-4 select-none",
            {
              "cursor-pointer": app.paramCount > 0,
            },
          )}
        >
          <div className={`${COLORS_CLASSES[app.color]} h-16 w-16 rounded p-2`}>
            {app.icon && (
              <Icon name={pascalToKebab(app.icon)} className="h-full w-full" />
            )}
          </div>
          <div className="flex-1">
            <p className="text-yellow-fp text-sm font-bold uppercase">App</p>
            <p className="text-lg font-medium">{app.name}</p>
          </div>
          <div className="flex-1">
            <p className="text-yellow-fp text-sm font-bold uppercase">
              {app.channels > 1 ? "Channels" : "Channel"}
            </p>
            <p className="text-lg font-medium">{getSlots(app)}</p>
          </div>
          <div className="flex-1">
            <p className="text-yellow-fp text-sm font-bold uppercase">Slots</p>
            <p className="text-lg font-medium">{app.channels}</p>
          </div>
          {app.paramCount > 0 ? (
            <div className="text-2xl group-open:rotate-90">
              <Icon className="h-7 w-7" name="caret" />
            </div>
          ) : (
            <div className="w-7" />
          )}
        </summary>
        {app.paramCount > 0 ? (
          <div>
            <div className="border-default-100 border-y-3 px-4 py-8">
              <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
                Parameters
              </h2>
              <div className="grid grid-cols-4 gap-x-16 gap-y-8 px-4">
                {!currentParamValues
                  ? app.params.map((_, idx) => (
                      <ParamSkeleton key={`param-${app.start}-${idx}`} />
                    ))
                  : app.params.map((param, idx) => (
                      <AppParam
                        key={`param-${app.start}-${idx}`}
                        param={param}
                        paramIndex={idx}
                        register={register}
                        defaultValue={getDefaultValue(currentParamValues[idx])}
                      />
                    ))}
              </div>
            </div>
            <div className="flex justify-end p-4">
              <ButtonPrimary
                disabled={isSubmitting}
                isLoading={isSubmitting}
                type="submit"
              >
                Save
              </ButtonPrimary>
            </div>
          </div>
        ) : null}
      </details>
    </form>
  );
};
