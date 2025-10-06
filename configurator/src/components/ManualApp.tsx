import { type Color } from "@atov/fp-config";

import clx from "classnames";

export interface ManualAppData {
  title: string;
  description: string;
  icon: string;
  color: Color["tag"];
  text: string;
  channels: Omit<ChannelProps, "idx">[];
}

interface FunctionFieldProps {
  title: string;
  description: string;
}

const FunctionField = ({ title, description }: FunctionFieldProps) => (
  <div>
    <div className="font-vox border-pink-fp border-b-1 px-2 py-0">{title}</div>
    <div className="px-2 text-xs italic">{description}</div>
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
      <span className="font-vox text-[35cqi] font-bold text-black">
        {label}
      </span>
    </div>
  </div>
);

interface ChannelProps {
  idx: number;
  jackTitle: string;
  jackDescription: string;
  faderTitle: string;
  faderDescription: string;
  faderPlusFnTitle: string;
  faderPlusFnDescription: string;
  faderPlusShiftTitle: string;
  faderPlusShiftDescription: string;
  fnTitle: string;
  fnDescription: string;
  fnPlusShiftTitle: string;
  fnPlusShiftDescription: string;
}

const Channel = ({
  idx,
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
}: ChannelProps) => (
  <>
    <div
      className="row-start-1 flex items-center justify-center"
      style={{ gridColumn: idx + 3 }}
    >
      <h1 className="font-vox text-xl font-semibold">Channel {idx + 1}</h1>
    </div>
    <div className="row-start-2" style={{ gridColumn: idx + 3 }}>
      <div className="h-3 rounded-t-full border-t border-r border-l"></div>
    </div>
    <div
      className="row-start-3 min-h-24 px-2 pb-2"
      style={{ gridColumn: idx + 3 }}
    >
      <FunctionField title={jackTitle} description={jackDescription} />
    </div>
    <div className="row-start-6 p-2" style={{ gridColumn: idx + 3 }}>
      <FunctionField title={faderTitle} description={faderDescription} />
    </div>
    {faderPlusFnTitle ? (
      <div className="row-start-7 p-2" style={{ gridColumn: idx + 3 }}>
        <FunctionField
          title={faderPlusFnTitle}
          description={faderPlusFnDescription}
        />
      </div>
    ) : null}
    {faderPlusShiftTitle ? (
      <div className="row-start-8 p-2" style={{ gridColumn: idx + 3 }}>
        <FunctionField
          title={faderPlusShiftTitle}
          description={faderPlusShiftDescription}
        />
      </div>
    ) : null}
    <div className="row-start-11 px-2 pt-8" style={{ gridColumn: idx + 3 }}>
      <FunctionField title={fnTitle} description={fnDescription} />
    </div>
    {fnPlusShiftTitle ? (
      <div className="row-start-12 p-2" style={{ gridColumn: idx + 3 }}>
        <FunctionField
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
    <div className="p-4">
      <h1 className="mb-4 text-2xl font-bold">Manual App</h1>
      <div
        className="inline-grid"
        style={{
          gridTemplateColumns: `auto auto repeat(${app.channels.length}, minmax(auto, 14rem))`,
        }}
      >
        <div
          className="col-start-1 flex flex-col items-center justify-start pt-2"
          style={{ gridRow: "3 / 11" }}
        >
          <img className="w-7" src="/img/jack.svg" />
          <img className="w-6" src="/img/fader-connect.svg" />
          <div className="relative flex w-full flex-1 justify-center">
            <div className="z-0 h-[calc(100%+0.5rem)] w-3 -translate-y-1 rounded bg-gradient-to-b from-gray-500 to-gray-300 p-[1px]">
              <div className="h-full w-full rounded bg-black">&nbsp;</div>
            </div>
          </div>
        </div>

        <div className="relative z-10 col-start-1 row-start-6 flex flex-col items-center justify-start pt-3">
          <img className="w-8" src="/img/fader-cap.svg" />
        </div>

        <div className="relative z-10 col-start-1 row-start-11 flex flex-col items-center justify-start">
          <img className="w-6 rotate-180" src="/img/fader-connect.svg" />
          <Button className="h-10 w-10" label="Fn" />
        </div>

        <div className="relative z-10 col-start-2 row-start-3 flex items-start justify-center pt-4">
          <img src="/img/arrow-bidirectional.svg" />
        </div>
        <div className="relative z-10 col-start-2 row-start-6 p-2">
          <div className="font-vox border-pink-fp border-b-1 px-2 py-0">
            &nbsp;
          </div>
          <div className="px-2 text-xs italic"></div>
        </div>
        {hasFaderPlusFn ? (
          <div className="relative z-10 col-start-2 row-start-7 flex items-start justify-center pt-4">
            <span className="text-pink-fp font-vox mr-1 text-2xl font-bold">
              +
            </span>
            <Button className="h-8 w-8" label="Fn" />
          </div>
        ) : null}
        {hasFaderPlusShift ? (
          <div className="relative z-10 col-start-2 row-start-8 flex items-start justify-center pt-4">
            <span className="text-pink-fp font-vox mr-1 text-2xl font-bold">
              +
            </span>
            <Button className="h-8 w-8" label="Shift" />
          </div>
        ) : null}
        <div className="relative z-10 col-start-2 row-start-11 px-2 pt-8">
          <div className="font-vox border-pink-fp border-b-1 px-2 py-0">
            &nbsp;
          </div>
          <div className="px-2 text-xs italic"></div>
        </div>
        {hasFnPlusShift ? (
          <div className="relative z-10 col-start-2 row-start-12 flex items-start justify-center pt-4">
            <span className="text-pink-fp font-vox mr-1 text-2xl font-bold">
              +
            </span>
            <Button className="h-8 w-8" label="Shift" />
          </div>
        ) : null}
        {app.channels.map((props, idx) => (
          <Channel key={idx} {...props} idx={idx} />
        ))}
      </div>
    </div>
  );
};
