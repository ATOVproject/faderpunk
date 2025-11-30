import { useState } from "react";
import { type Value } from "@atov/fp-config";
import { useForm } from "react-hook-form";
import classNames from "classnames";

import { COLORS_CLASSES } from "../utils/class-helpers";
import {
  pascalToKebab,
  getDefaultValue,
  getSlots,
  transformParamFormValues,
} from "../utils/utils";
import { ButtonPrimary } from "./Button";
import { Icon } from "./Icon";
import type { App } from "../utils/types";
import { setAppParams } from "../utils/config.ts";
import { useStore } from "../store.ts";
import { AppParam } from "./input/AppParam.tsx";

interface Props {
  app: App;
  layoutId: number;
  startChannel: number;
  params: Value[];
}

export const ActiveApp = ({ app, layoutId, params, startChannel }: Props) => {
  const { usbDevice, setParams } = useStore();
  const [saved, setSaved] = useState<boolean>(false);
  const {
    register,
    control,
    handleSubmit,
    formState: { isSubmitting },
  } = useForm();

  const onSubmit = async (
    data: Record<string, string | boolean | boolean[]>,
  ) => {
    if (usbDevice) {
      const values = transformParamFormValues(data);
      const params = await setAppParams(usbDevice, layoutId, values);
      setParams(layoutId, params);
      setSaved(true);
      setTimeout(() => {
        setSaved(false);
      }, 2000);
    }
  };

  return (
    <form onSubmit={handleSubmit(onSubmit)}>
      <details className="group w-full bg-black">
        <summary
          className={classNames(
            "flex list-none items-center gap-4 p-4 select-none",
            {
              "cursor-pointer": app.paramCount > 0,
            },
          )}
        >
          <div
            className={`${COLORS_CLASSES[app.color].bg} h-16 w-16 rounded p-2`}
          >
            {app.icon && (
              <Icon
                name={pascalToKebab(app.icon)}
                className="h-full w-full text-black"
              />
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
            <p className="text-lg font-medium">{getSlots(app, startChannel)}</p>
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
              <div className="grid grid-cols-4 gap-x-8 gap-y-8 px-4">
                {app.params.map((param, idx) => (
                  <AppParam
                    key={`param-${startChannel}-${idx}`}
                    param={param}
                    paramIndex={idx}
                    register={register}
                    control={control}
                    defaultValue={getDefaultValue(params[idx])}
                  />
                ))}
              </div>
            </div>
            <div className="flex justify-end p-4">
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
          </div>
        ) : null}
      </details>
    </form>
  );
};
