import { useEffect, useMemo, useRef, useState } from "react";
import { Checkbox } from "@heroui/checkbox";
import ReactMarkdown from "react-markdown";

import { useStore } from "../store";
import {
  abiHex,
  cacheFpAppManual,
  getFpAppSlots,
  getFpAppSupport,
  installFpApp,
  parseFpApp,
  removeCachedFpAppManual,
  removeFpApp,
  type FpAppSlot,
  type FpAppSupport,
  type ParsedFpApp,
} from "../utils/fpapp";
import { ButtonPrimary, ButtonSecondary } from "./Button";

interface Selection {
  app: ParsedFpApp;
  slot: number;
}

const EMPTY_SLOTS: FpAppSlot[] = Array.from({ length: 4 }, (_, slot) => ({
  slot,
}));

export const InstalledApps = () => {
  const {
    device,
    isSimulator,
    refreshApps,
    addSimulatorApp,
    removeSimulatorApp,
    setSuspendHealthCheck,
  } = useStore();
  const [support, setSupport] = useState<FpAppSupport>();
  const [slots, setSlots] = useState<FpAppSlot[]>(EMPTY_SLOTS);
  const [selection, setSelection] = useState<Selection>();
  const [trusted, setTrusted] = useState(false);
  const [progress, setProgress] = useState<number>();
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(Boolean(device));
  const [error, setError] = useState<string>();
  const [notice, setNotice] = useState<string>();
  const [removalSlot, setRemovalSlot] = useState<number>();
  const [simulatorPackages, setSimulatorPackages] = useState(
    new Map<number, ParsedFpApp>(),
  );
  const sectionRef = useRef<HTMLDivElement>(null);

  const refresh = async (showLoading = true) => {
    if (!device) return;
    if (showLoading) setLoading(true);
    try {
      // The config transport supports one in-flight receive per MIDI device.
      // Keep these queries sequential so the slot batch cannot consume the
      // support response (or vice versa).
      const nextSupport = await getFpAppSupport(device);
      const nextSlots = await getFpAppSlots(device);
      setSupport(nextSupport);
      setSlots(nextSlots.sort((a, b) => a.slot - b.slot));
      setError(undefined);
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      if (showLoading) setLoading(false);
    }
  };

  const preserveSectionPosition = async (
    update: () => void | Promise<void>,
  ) => {
    const previousTop = sectionRef.current?.getBoundingClientRect().top;
    await update();
    requestAnimationFrame(() => {
      if (previousTop === undefined || !sectionRef.current) return;
      const shift =
        sectionRef.current.getBoundingClientRect().top - previousTop;
      if (Math.abs(shift) > 0.5) window.scrollBy(0, shift);
    });
  };

  useEffect(() => {
    void refresh();
    // The selected MIDI device is the lifecycle for this device-owned view.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [device]);

  const firmwareAbi = support ? abiHex(support.firmware_abi) : undefined;
  const isCompatible = useMemo(
    () =>
      selection && firmwareAbi
        ? selection.app.firmwareAbi === firmwareAbi
        : Boolean(selection && isSimulator),
    [firmwareAbi, isSimulator, selection],
  );

  const chooseFile = async (file: File, slot: number) => {
    setError(undefined);
    setNotice(undefined);
    setTrusted(false);
    setRemovalSlot(undefined);
    try {
      const app = await parseFpApp(file);
      setSelection({ app, slot });
    } catch (caught) {
      setSelection(undefined);
      setError(errorMessage(caught));
    }
  };

  const install = async () => {
    if (!selection || !isCompatible || !trusted) return;
    setBusy(true);
    setError(undefined);
    setNotice(undefined);
    setProgress(0);
    setSuspendHealthCheck(true);
    try {
      if (device && support) {
        await installFpApp(
          device,
          selection.slot,
          selection.app,
          support,
          setProgress,
        );
        await refresh(false);
        await preserveSectionPosition(refreshApps);
      } else if (isSimulator) {
        const nextPackages = new Map(simulatorPackages).set(
          selection.slot,
          selection.app,
        );
        setSimulatorPackages(nextPackages);
        cacheFpAppManual(selection.app);
        await preserveSectionPosition(() =>
          addSimulatorApp({
            appId: selection.app.appId,
            channels: BigInt(selection.app.channels),
            paramCount: BigInt(selection.app.appConfig?.params.length ?? 0),
            name: selection.app.name,
            description: selection.app.description,
            color: selection.app.appConfig?.color ?? "White",
            icon: selection.app.appConfig?.icon ?? "Fader",
            params: selection.app.appConfig?.params ?? [],
          }),
        );
        setSlots((current) =>
          current.map((slot) =>
            slot.slot === selection.slot
              ? {
                  slot: slot.slot,
                  app: {
                    slot: slot.slot,
                    app_id: selection.app.appId,
                    version_major: Number(selection.app.version.split(".")[0]),
                    version_minor: Number(selection.app.version.split(".")[1]),
                    version_patch: Number(selection.app.version.split(".")[2]),
                    channels: selection.app.channels,
                    name: selection.app.name,
                    description: selection.app.description,
                    author: selection.app.author,
                    has_manual: Boolean(selection.app.manual),
                    has_setup: Boolean(selection.app.setup),
                    has_settings: Boolean(selection.app.settings),
                    signed: selection.app.signed,
                  },
                }
              : slot,
          ),
        );
        setProgress(1);
      }
      setNotice(
        `${selection.app.name} is installed in slot ${selection.slot + 1}.`,
      );
      setSelection(undefined);
      setTrusted(false);
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setSuspendHealthCheck(false);
      setBusy(false);
      setProgress(undefined);
    }
  };

  const remove = async (slot: FpAppSlot) => {
    if (!slot.app) return;
    setBusy(true);
    setError(undefined);
    setSuspendHealthCheck(true);
    try {
      if (device) {
        await removeFpApp(device, slot.slot);
        await refresh(false);
        await preserveSectionPosition(refreshApps);
      } else {
        await preserveSectionPosition(() =>
          removeSimulatorApp(slot.app!.app_id),
        );
        setSlots((current) =>
          current.map((item) =>
            item.slot === slot.slot ? { slot: item.slot } : item,
          ),
        );
        const nextPackages = new Map(simulatorPackages);
        nextPackages.delete(slot.slot);
        setSimulatorPackages(nextPackages);
      }
      removeCachedFpAppManual(slot.app.app_id);
      setNotice(`Slot ${slot.slot + 1} is ready for another app.`);
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setRemovalSlot(undefined);
      setSuspendHealthCheck(false);
      setBusy(false);
    }
  };

  return (
    <div className="border-t border-white/10 pt-10 pb-12" ref={sectionRef}>
      <div className="mb-6 max-w-3xl">
        <h2 className="text-yellow-fp mb-2 text-xl font-bold">
          Installed Apps
        </h2>
        <div className="min-h-6 text-sm leading-6">
          {error ? (
            <p className="text-red-300" role="alert">
              {error}
            </p>
          ) : notice ? (
            <p className="text-green-300" role="status">
              {notice}
            </p>
          ) : (
            <p className="text-gray-300">
              Install a community app in any empty slot.
            </p>
          )}
        </div>
      </div>

      <div className="overflow-hidden rounded-sm bg-black">
        <div className="hidden grid-cols-[5rem_1fr_auto] border-b border-white/10 px-5 py-3 text-xs font-bold text-gray-400 uppercase md:grid">
          <span>Slot</span>
          <span>Installed app</span>
          <span className="min-w-64 text-right">Actions</span>
        </div>
        {loading
          ? Array.from({ length: 4 }, (_, index) => (
              <div
                className="grid min-h-28 animate-pulse grid-cols-[4rem_1fr] items-center gap-4 border-b border-white/10 px-5 py-4 last:border-b-0"
                key={index}
              >
                <div className="h-8 w-8 rounded-sm bg-white/10" />
                <div className="h-5 max-w-sm rounded-sm bg-white/10" />
              </div>
            ))
          : slots.map((slot) => (
              <div
                className="border-b border-white/10 last:border-b-0"
                key={slot.slot}
              >
                <div className="grid gap-4 px-5 py-5 md:min-h-28 md:grid-cols-[5rem_1fr_auto] md:items-center">
                  <div className="flex items-center gap-3">
                    <span className="text-yellow-fp text-lg font-bold tabular-nums">
                      {slot.slot + 1}
                    </span>
                    <span
                      className={`h-2 w-2 rounded-full ${slot.app ? "bg-green-400" : "bg-gray-600"}`}
                      aria-label={slot.app ? "Installed" : "Empty"}
                    />
                  </div>
                  {slot.app ? (
                    <div>
                      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                        <h3 className="font-bold text-white">
                          {slot.app.name}
                        </h3>
                        <span className="text-xs text-gray-400">
                          v{slot.app.version_major}.{slot.app.version_minor}.
                          {slot.app.version_patch}
                        </span>
                      </div>
                      <p className="mt-1 text-sm text-gray-300">
                        {slot.app.description}
                      </p>
                      <p className="mt-1 text-xs text-gray-400">
                        by {slot.app.author}
                      </p>
                    </div>
                  ) : (
                    <h3 className="font-bold text-gray-400">Empty</h3>
                  )}
                  <div className="flex flex-wrap gap-2 md:min-w-64 md:justify-end">
                    {removalSlot === slot.slot && slot.app ? (
                      <div
                        className="flex flex-wrap items-center justify-end gap-2"
                        role="group"
                        aria-label={`Remove ${slot.app.name} from slot ${slot.slot + 1}?`}
                      >
                        <ButtonSecondary
                          size="sm"
                          isDisabled={busy}
                          onPress={() => setRemovalSlot(undefined)}
                        >
                          Keep
                        </ButtonSecondary>
                        <ButtonPrimary
                          className="min-w-36 text-white"
                          color="danger"
                          size="sm"
                          isDisabled={busy}
                          isLoading={busy}
                          onPress={() => void remove(slot)}
                        >
                          Confirm remove
                        </ButtonPrimary>
                      </div>
                    ) : (
                      <>
                        <label
                          className="bg-primary hover:bg-primary-400 focus-within:ring-primary cursor-pointer rounded-sm px-4 py-2 text-sm font-semibold text-black focus-within:ring-2 focus-within:ring-offset-2 focus-within:ring-offset-black"
                          htmlFor={`fpapp-slot-${slot.slot}`}
                        >
                          {slot.app ? "Replace" : "Install"}
                        </label>
                        <input
                          className="sr-only"
                          id={`fpapp-slot-${slot.slot}`}
                          type="file"
                          accept=".fpapp,application/octet-stream"
                          disabled={busy}
                          onChange={(event) => {
                            const file = event.target.files?.[0];
                            if (file) void chooseFile(file, slot.slot);
                            event.currentTarget.value = "";
                          }}
                        />
                        {slot.app && (
                          <ButtonSecondary
                            size="sm"
                            isDisabled={busy}
                            onPress={() => {
                              setSelection(undefined);
                              setTrusted(false);
                              setRemovalSlot(slot.slot);
                            }}
                          >
                            Remove
                          </ButtonSecondary>
                        )}
                      </>
                    )}
                  </div>
                </div>

                {selection?.slot === slot.slot && (
                  <div className="bg-zinc-950 px-5 py-6 md:pl-25">
                    <div className="max-w-3xl">
                      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                        <h3 className="text-lg font-bold text-white">
                          {selection.app.name}
                        </h3>
                        <span className="text-sm text-gray-400">
                          v{selection.app.version} by {selection.app.author}
                        </span>
                      </div>
                      <p className="mt-2 text-sm leading-6 text-gray-300">
                        {selection.app.description}
                      </p>
                      {selection.app.setup && (
                        <div className="prose prose-invert prose-sm mt-5 text-gray-200">
                          <ReactMarkdown>{selection.app.setup}</ReactMarkdown>
                        </div>
                      )}
                      {!isCompatible && (
                        <p className="mt-5 text-sm text-red-300" role="alert">
                          This app needs a different firmware version.
                        </p>
                      )}
                    </div>

                    <div className="mt-6 max-w-3xl border-t border-white/10 pt-5">
                      <Checkbox
                        isSelected={trusted}
                        onValueChange={setTrusted}
                        classNames={{
                          label: "text-sm leading-5 text-gray-200",
                        }}
                      >
                        I trust this app and its source.
                      </Checkbox>
                      <div className="mt-5 flex flex-wrap gap-3">
                        <ButtonPrimary
                          className="min-w-48"
                          isDisabled={!trusted || !isCompatible || busy}
                          isLoading={busy}
                          onPress={() => void install()}
                        >
                          {busy && progress !== undefined
                            ? `Installing · ${Math.round(progress * 100)}%`
                            : `Install in slot ${selection.slot + 1}`}
                        </ButtonPrimary>
                        <ButtonSecondary
                          isDisabled={busy}
                          onPress={() => {
                            setSelection(undefined);
                            setTrusted(false);
                          }}
                        >
                          Cancel
                        </ButtonSecondary>
                      </div>
                      <div
                        className="mt-3 h-1 overflow-hidden rounded-full bg-white/10"
                        aria-label={
                          progress === undefined
                            ? undefined
                            : `Uploading to slot ${selection.slot + 1}: ${Math.round(progress * 100)}%`
                        }
                        aria-valuemin={progress === undefined ? undefined : 0}
                        aria-valuemax={progress === undefined ? undefined : 100}
                        aria-valuenow={
                          progress === undefined
                            ? undefined
                            : Math.round(progress * 100)
                        }
                        role={
                          progress === undefined ? undefined : "progressbar"
                        }
                      >
                        <div
                          className={`bg-yellow-fp h-full transition-[width,opacity] duration-200 motion-reduce:transition-none ${progress === undefined ? "opacity-0" : "opacity-100"}`}
                          style={{ width: `${(progress ?? 0) * 100}%` }}
                        />
                      </div>
                    </div>
                  </div>
                )}
              </div>
            ))}
      </div>
    </div>
  );
};

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
