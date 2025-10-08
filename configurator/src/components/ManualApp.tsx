import { type ReactNode } from "react";
import clx from "classnames";

import { COLORS_CLASSES } from "../utils/class-helpers";
import { Icon } from "./Icon";
import { AllColors } from "../utils/types";

interface ArrowIconProps {
  className?: string;
}

const ArrowIcon = ({ className }: ArrowIconProps) => (
  <svg
    className={className}
    width="43.3404"
    height="10.8268"
    viewBox="0 0 43.3404 10.8268"
  >
    <g>
      <path
        d="M13.223,3.9317h29.7638c-1.1399-1.1399-3.5782-3.5782-3.5782-3.5782"
        fill="none"
        stroke="currentColor"
        strokeMiterlimit="1.5"
      />
      <path
        d="M3.9768,10.4732L.3536,6.85c8.1959,0,29.7638,0,29.7638,0"
        fill="none"
        stroke="currentColor"
        strokeMiterlimit="1.5"
      />
    </g>
  </svg>
);

export interface ManualAppData {
  appId: number;
  title: string;
  description: ReactNode;
  icon: string;
  color: AllColors;
  params?: string[];
  text: ReactNode;
  channels: Omit<ChannelProps, "idx" | "color" | "singleChannel">[];
}

interface FunctionFieldProps {
  color: AllColors;
  title: string;
  description?: ReactNode;
}

const FunctionField = ({ color, title, description }: FunctionFieldProps) => (
  <div>
    <div
      className={clx(
        COLORS_CLASSES[color].border,
        "border-b-1 px-2 py-0 font-semibold",
      )}
    >
      {title}
    </div>
    <div className="px-2 text-sm italic">{description}</div>
  </div>
);

interface ButtonProps {
  className: string;
  label: string;
}

const Button = ({ className, label }: ButtonProps) => (
  <div className={clx(className, "@container relative")}>
    <img className="absolute h-full w-full" src="/img/button.svg" />
    <div className="absolute flex h-full w-full items-center justify-center">
      <span className="font-vox translate-y-0.5 text-[35cqi] font-bold text-black">
        {label}
      </span>
    </div>
  </div>
);

interface ChannelProps {
  idx: number;
  color: AllColors;
  jackTitle: string;
  jackDescription: ReactNode;
  faderTitle: string;
  faderDescription: ReactNode;
  faderPlusFnTitle?: string;
  faderPlusFnDescription?: ReactNode;
  faderPlusShiftTitle?: string;
  faderPlusShiftDescription?: ReactNode;
  fnTitle: string;
  fnDescription: string;
  fnPlusShiftTitle?: string;
  fnPlusShiftDescription?: ReactNode;
  ledTop: ReactNode;
  ledTopPlusShift?: ReactNode;
  ledTopPlusFn?: ReactNode;
  ledBottom: ReactNode;
  ledBottomPlusShift?: ReactNode;
  ledBottomPlusFn?: ReactNode;
  singleChannel: boolean;
}

const Channel = ({
  idx,
  color,
  jackTitle,
  jackDescription,
  faderTitle,
  faderDescription,
  faderPlusFnTitle,
  faderPlusFnDescription,
  faderPlusShiftTitle,
  faderPlusShiftDescription,
  fnTitle,
  fnDescription,
  fnPlusShiftTitle,
  fnPlusShiftDescription,
  ledTop,
  ledTopPlusShift,
  ledTopPlusFn,
  ledBottom,
  ledBottomPlusShift,
  ledBottomPlusFn,
  singleChannel,
}: ChannelProps) => (
  <>
    {!singleChannel ? (
      <div
        className="row-start-1 flex items-center justify-center"
        style={{ gridColumn: idx + 3 }}
      >
        <h1 className="font-vox font-semibold">Channel {idx + 1}</h1>
      </div>
    ) : null}
    {!singleChannel ? (
      <div className="row-start-2" style={{ gridColumn: idx + 3 }}>
        <div className="h-3 rounded-t-full border-t border-r border-l"></div>
      </div>
    ) : null}
    <div className="row-start-3 px-2 pb-4" style={{ gridColumn: idx + 3 }}>
      <FunctionField
        color={color}
        title={jackTitle}
        description={jackDescription}
      />
    </div>
    <div className="row-start-4 pt-1 pb-4" style={{ gridColumn: idx + 3 }}>
      <div className="px-2 text-sm italic">{ledTop}</div>
      {ledTopPlusShift ? (
        <div className="px-2 text-sm italic">
          <span className="font-vox font-semibold">Shift:</span>{" "}
          {ledTopPlusShift}
        </div>
      ) : null}
      {ledTopPlusFn ? (
        <div className="px-2 text-sm italic">
          <span className="font-vox font-semibold">Fn:</span> {ledTopPlusFn}
        </div>
      ) : null}
    </div>
    <div className="row-start-6 p-2" style={{ gridColumn: idx + 3 }}>
      <FunctionField
        color={color}
        title={faderTitle}
        description={faderDescription}
      />
    </div>
    {faderPlusFnTitle ? (
      <div className="row-start-7 p-2" style={{ gridColumn: idx + 3 }}>
        <FunctionField
          color={color}
          title={faderPlusFnTitle}
          description={faderPlusFnDescription}
        />
      </div>
    ) : null}
    {faderPlusShiftTitle ? (
      <div className="row-start-8 p-2" style={{ gridColumn: idx + 3 }}>
        <FunctionField
          color={color}
          title={faderPlusShiftTitle}
          description={faderPlusShiftDescription}
        />
      </div>
    ) : null}
    <div className="row-start-9 pt-1" style={{ gridColumn: idx + 3 }}>
      <div className="px-2 text-sm italic">{ledBottom}</div>
      {ledBottomPlusShift ? (
        <div className="px-2 text-sm italic">
          <span className="font-vox font-semibold">Shift:</span>{" "}
          {ledBottomPlusShift}
        </div>
      ) : null}
      {ledBottomPlusFn ? (
        <div className="px-2 text-sm italic">
          <span className="font-vox font-semibold">Fn:</span> {ledBottomPlusFn}
        </div>
      ) : null}
    </div>
    <div className="row-start-11 px-2 pt-6" style={{ gridColumn: idx + 3 }}>
      <FunctionField
        color={color}
        title={fnTitle}
        description={fnDescription}
      />
    </div>
    {fnPlusShiftTitle ? (
      <div className="row-start-12 p-2" style={{ gridColumn: idx + 3 }}>
        <FunctionField
          color={color}
          title={fnPlusShiftTitle}
          description={fnPlusShiftDescription}
        />
      </div>
    ) : null}
  </>
);

interface Props {
  app: ManualAppData;
}

export const ManualApp = ({ app }: Props) => {
  const hasFaderPlusShift = app.channels.some(
    (chan) => !!chan.faderPlusShiftTitle,
  );
  const hasFaderPlusFn = app.channels.some((chan) => !!chan.faderPlusFnTitle);
  const hasFnPlusShift = app.channels.some((chan) => !!chan.fnPlusShiftTitle);

  return (
    <div id={`app-${app.appId}`}>
      <div className="mb-4 flex gap-4">
        <div
          className={clx(
            "flex items-center justify-center rounded-sm p-2",
            COLORS_CLASSES[app.color].bg,
          )}
        >
          <Icon className="h-12 w-12 text-black" name={app.icon} />
        </div>
        <div>
          <h3 className="text-yellow-fp font-bold uppercase">{app.title}</h3>
          <p>{app.description}</p>
        </div>
      </div>
      {app.params ? (
        <div className="mb-4">
          <h3 className="text-sm font-bold uppercase">
            Available app parameters
          </h3>
          <ul className="list-inside list-disc">
            {app.params.map((param) => (
              <li className="text-sm" key={param}>
                {param}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
      <p className="mb-4">{app.text}</p>
      <div
        className="inline-grid"
        style={{
          gridTemplateColumns: `auto auto repeat(${app.channels.length}, minmax(auto, 14rem))`,
        }}
      >
        <div className="relative z-10 col-start-1 row-start-3 flex flex-col items-center justify-start pt-2">
          <img className="w-7" src="/img/jack.svg" />
          <div className="relative flex-1">
            <div className="absolute top-0 left-1/2 h-full w-[1.5px] -translate-x-1/2 bg-white"></div>
          </div>
          <span className="border-t-1.5 border-r-1.5 border-l-1.5 rounded-t-large h-3 w-6 border-white" />
        </div>
        <div
          className="col-start-1 flex flex-col items-center justify-start"
          style={{ gridRow: "4 / 11" }}
        >
          <div className="relative flex w-full flex-1 justify-center">
            <div className="z-0 h-[calc(100%+0.5rem)] w-3 -translate-y-1 rounded bg-gradient-to-b from-gray-500 to-gray-300 p-[1px]">
              <div className="h-full w-full rounded bg-black">&nbsp;</div>
            </div>
          </div>
        </div>

        <div className="relative z-10 col-start-1 row-start-6 flex flex-col items-center justify-start pt-3">
          <img className="w-8" src="/img/fader-cap.svg" />
        </div>
        <div className="relative z-10 col-start-1 row-start-3 flex flex-col items-center justify-start pt-2">
          <img className="w-7" src="/img/jack.svg" />
          <div className="relative flex-1">
            <div className="absolute top-0 left-1/2 h-full w-[1.5px] -translate-x-1/2 bg-white"></div>
          </div>
          <span className="border-t-1.5 border-r-1.5 border-l-1.5 rounded-t-large h-3 w-6 border-white" />
        </div>

        <div className="relative z-10 col-start-1 row-start-11 flex flex-col items-center justify-start">
          <span className="border-b-1.5 border-r-1.5 border-l-1.5 rounded-b-large h-3 w-6 border-white" />
          <div className="relative flex-1">
            <div className="absolute top-0 left-1/2 h-full w-[1.5px] -translate-x-1/2 bg-white"></div>
          </div>
          <Button className="h-10 w-10" label="Fn" />
        </div>

        <div className="z-10 col-start-2 row-start-3 flex items-start justify-center pt-4">
          {/* <img src="/img/arrow-bidirectional.svg" /> */}
          <ArrowIcon className={COLORS_CLASSES[app.color].text} />
        </div>
        <div className="z-10 col-start-2 row-start-4 flex items-start pt-2 pr-2">
          <div className="flex flex-1 items-center">
            <img src="/img/led.svg" />
            <div className="relative flex-1">
              <div
                className={clx(
                  COLORS_CLASSES[app.color].bg,
                  "absolute top-[calc(50%-0.5px)] left-0 ml-1 h-px w-[calc(100%-0.25rem)]",
                )}
              ></div>
            </div>
          </div>
        </div>
        <div className="relative z-10 col-start-2 row-start-6 p-2">
          <div
            className={clx(
              COLORS_CLASSES[app.color].border,
              "font-vox border-b-1 px-2 py-0",
            )}
          >
            &nbsp;
          </div>
          <div className="px-2 text-xs italic"></div>
        </div>
        {hasFaderPlusFn ? (
          <div className="relative z-10 col-start-2 row-start-7 flex items-start justify-center pt-4">
            <span
              className={clx(
                COLORS_CLASSES[app.color].text,
                "font-vox mr-1 text-2xl font-semibold",
              )}
            >
              +
            </span>
            <Button className="h-8 w-8" label="Fn" />
          </div>
        ) : null}
        {hasFaderPlusShift ? (
          <div className="relative z-10 col-start-2 row-start-8 flex items-start justify-center pt-4">
            <span
              className={clx(
                COLORS_CLASSES[app.color].text,
                "font-vox mr-1 text-2xl font-semibold",
              )}
            >
              +
            </span>
            <Button className="h-8 w-8" label="Shift" />
          </div>
        ) : null}
        <div className="z-10 col-start-2 row-start-9 flex items-start pt-2 pr-2 pb-2">
          <div className="flex flex-1 items-center">
            <img src="/img/led.svg" />
            <div className="relative flex-1">
              <div
                className={clx(
                  COLORS_CLASSES[app.color].bg,
                  "absolute top-[calc(50%-0.5px)] left-0 ml-1 h-px w-[calc(100%-0.25rem)]",
                )}
              ></div>
            </div>
          </div>
        </div>
        <div className="relative z-10 col-start-2 row-start-11 px-2 pt-6">
          <div
            className={clx(
              COLORS_CLASSES[app.color].border,
              "font-vox border-b-1 px-2 py-0",
            )}
          >
            &nbsp;
          </div>
          <div className="px-2 text-xs italic"></div>
        </div>
        {hasFnPlusShift ? (
          <div className="relative z-10 col-start-2 row-start-12 flex items-start justify-center pt-4">
            <span
              className={clx(
                COLORS_CLASSES[app.color].text,
                "font-vox mr-1 text-2xl font-semibold",
              )}
            >
              +
            </span>
            <Button className="h-8 w-8" label="Shift" />
          </div>
        ) : null}
        {app.channels.map((props, idx) => (
          <Channel
            key={idx}
            {...props}
            idx={idx}
            color={app.color}
            singleChannel={app.channels.length === 1}
          />
        ))}
      </div>
    </div>
  );
};
