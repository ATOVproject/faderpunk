import { useCallback, useState } from "react";
import { type Value } from "@atov/fp-config";
import { Skeleton } from "@heroui/skeleton";
import { useForm } from "react-hook-form";
import classNames from "classnames";

import { COLORS_CLASSES } from "../utils/class-helpers";
import { pascalToKebab, getDefaultValue, getSlots } from "../utils/utils";
import { ButtonPrimary } from "./Button";
import { Icon } from "./Icon";
import type { App } from "../utils/types";
import { getAppParams, setAppParams } from "../utils/config.ts";
import { useStore } from "../store.ts";
import { AppParam } from "./input/AppParam.tsx";

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

interface Props {
  app: App;
  layoutId: number;
  startChannel: number;
}

export const ActiveApp = ({ app, layoutId, startChannel }: Props) => {
  const { usbDevice } = useStore();
  const [hasBeenOpened, setHasBeenOpened] = useState<boolean>(false);
  const [currentParamValues, setParams] = useState<Value[]>();
  const [saved, setSaved] = useState<boolean>(false);
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
          const params = await getAppParams(usbDevice, layoutId);
          setParams(params);
        }
      }
    },
    [hasBeenOpened, usbDevice, layoutId],
  );

  const onSubmit = async (data: Record<string, string | boolean>) => {
    if (usbDevice) {
      const params = await setAppParams(usbDevice, layoutId, data);
      setParams(params);
      setSaved(true);
      setTimeout(() => {
        setSaved(false);
      }, 2000);
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
              <div className="grid grid-cols-4 gap-x-16 gap-y-8 px-4">
                {!currentParamValues
                  ? app.params.map((_, idx) => (
                      <ParamSkeleton key={`param-${startChannel}-${idx}`} />
                    ))
                  : app.params.map((param, idx) => (
                      <AppParam
                        key={`param-${startChannel}-${idx}`}
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
