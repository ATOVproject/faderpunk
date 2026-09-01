import { useState } from "react";
import { Button } from "@heroui/button";
import {
  Modal,
  ModalBody,
  ModalContent,
  ModalFooter,
  ModalHeader,
} from "@heroui/modal";
import { Select, SelectItem } from "@heroui/select";
import { Slider } from "@heroui/slider";
import { Switch } from "@heroui/switch";
import { Tabs, Tab } from "@heroui/tabs";
import { useStore } from "../store";
import { routing } from "@atov/fp-config";

type DestCategory = "PhysicalDac" | "AppInput" | "AppFader";

const COMBINE_MODES: { label: string; modeTag: routing.CombineMode["tag"] }[] =
  [
    { label: "Sum (A + B)", modeTag: "Sum" },
    { label: "Average ((A + B) / 2)", modeTag: "Average" },
    { label: "Max (Highest)", modeTag: "Max" },
    { label: "Min (Lowest)", modeTag: "Min" },
    { label: "Logic OR", modeTag: "Or" },
    { label: "Logic AND", modeTag: "And" },
    { label: "Logic XOR", modeTag: "Xor" },
    { label: "Replace / Override", modeTag: "Replace" },
  ];

export const PatchbayTab = () => {
  const { routing: routingState, setRouting, layout } = useStore();
  const [destCategory, setDestCategory] = useState<DestCategory>("PhysicalDac");
  const [selectedRouteIdx, setSelectedRouteIdx] = useState<number | null>(null);
  const [editingRoute, setEditingRoute] = useState<routing.Route | null>(null);

  const routeList = Array.from(routingState?.routes || []) as (
    | routing.Route
    | undefined
  )[];

  const getAppName = (channel: number) => {
    if (!layout) return `Ch ${channel + 1}`;
    const slot = layout.find((s) => s.startChannel === channel);
    return slot?.app
      ? `${slot.app.name} (Ch ${channel + 1})`
      : `Ch ${channel + 1}`;
  };

  const isSameSource = (a: routing.RouteSource, b: routing.RouteSource) => {
    if (a.tag === "AppOutput" && b.tag === "AppOutput")
      return a.value.channel === b.value.channel;
    if (a.tag === "PhysicalAdc" && b.tag === "PhysicalAdc")
      return a.value.channel === b.value.channel;
    if (a.tag === "AppMidi" && b.tag === "AppMidi")
      return a.value.channel === b.value.channel;
    if (a.tag === "Constant" && b.tag === "Constant")
      return a.value.value === b.value.value;
    return false;
  };

  const isSameDest = (
    a: routing.RouteDestination,
    b: routing.RouteDestination,
  ) => {
    if (a.tag === "AppInput" && b.tag === "AppInput")
      return a.value.channel === b.value.channel;
    if (a.tag === "PhysicalDac" && b.tag === "PhysicalDac")
      return a.value.channel === b.value.channel;
    if (a.tag === "AppFader" && b.tag === "AppFader")
      return a.value.channel === b.value.channel;
    if (a.tag === "AppLayer" && b.tag === "AppLayer")
      return a.value.channel === b.value.channel;
    return false;
  };

  const handleCellClick = (
    src: routing.RouteSource,
    dest: routing.RouteDestination,
  ) => {
    const existingIdx = routeList.findIndex(
      (r) =>
        r && isSameSource(r.source, src) && isSameDest(r.destination, dest),
    );

    if (existingIdx >= 0 && routeList[existingIdx]) {
      setSelectedRouteIdx(existingIdx);
      setEditingRoute({ ...routeList[existingIdx]! });
    } else {
      const emptyIdx = routeList.findIndex((r) => !r);
      const targetIdx = emptyIdx >= 0 ? emptyIdx : 0;
      setSelectedRouteIdx(targetIdx);
      setEditingRoute({
        source: src,
        destination: dest,
        mode: { tag: "Sum" },
        attenuation_percent: 100,
        offset: 0,
        enabled: true,
      });
    }
  };

  const handleSaveRoute = () => {
    if (selectedRouteIdx === null || !editingRoute) return;
    const newRoutes = [...routeList];
    newRoutes[selectedRouteIdx] = editingRoute;
    setRouting({ routes: newRoutes as unknown } as routing.RoutingConfig);
    setSelectedRouteIdx(null);
    setEditingRoute(null);
  };

  const handleDeleteRoute = (idx: number) => {
    const newRoutes = [...routeList];
    newRoutes[idx] = undefined;
    setRouting({ routes: newRoutes as unknown } as routing.RoutingConfig);
  };

  const formatSourceLabel = (src: routing.RouteSource) => {
    if (src.tag === "AppOutput")
      return `App Out: ${getAppName(src.value.channel)}`;
    if (src.tag === "PhysicalAdc") return `ADC In: Ch ${src.value.channel + 1}`;
    if (src.tag === "Constant") return `Const: ${src.value.value}`;
    return "Source";
  };

  const formatDestLabel = (dest: routing.RouteDestination) => {
    if (dest.tag === "PhysicalDac")
      return `Physical CV Out: Jack ${dest.value.channel + 1}`;
    if (dest.tag === "AppInput")
      return `App Input: ${getAppName(dest.value.channel)}`;
    if (dest.tag === "AppFader")
      return `Fader Mod: Ch ${dest.value.channel + 1}`;
    return "Destination";
  };

  const createDestForCategory = (
    category: DestCategory,
    destChan: number,
  ): routing.RouteDestination => {
    switch (category) {
      case "PhysicalDac":
        return { tag: "PhysicalDac", value: { channel: destChan } };
      case "AppInput":
        return { tag: "AppInput", value: { channel: destChan } };
      case "AppFader":
        return { tag: "AppFader", value: { channel: destChan } };
    }
  };

  const getColHeaderLabel = (category: DestCategory, destChan: number) => {
    switch (category) {
      case "PhysicalDac":
        return `CV Out ${destChan + 1}`;
      case "AppInput":
        return `App ${destChan + 1} In`;
      case "AppFader":
        return `Fad ${destChan + 1} Mod`;
    }
  };

  const activeCount = routeList.filter((r) => Boolean(r)).length;

  return (
    <div className="flex flex-col gap-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold tracking-tight text-white">
            Internal Digital Patchbay
          </h2>
          <p className="text-sm text-neutral-400">
            Digitally connect, combine, or modulate signals between apps,
            physical CV outputs, and faders.
          </p>
        </div>
        <Button
          color="danger"
          variant="flat"
          size="sm"
          onPress={() =>
            setRouting({
              routes: new Array(32).fill(undefined) as unknown,
            } as routing.RoutingConfig)
          }
        >
          Clear All Routes
        </Button>
      </div>

      {/* Target Category Selector Tabs */}
      <div className="flex flex-col gap-3 rounded-xl border border-neutral-800 bg-neutral-900/60 p-4 backdrop-blur-md">
        <div className="flex items-center justify-between">
          <div>
            <h3 className="text-lg font-semibold text-white">
              Select Destination Target Type
            </h3>
            <p className="text-xs text-neutral-400">
              Choose what destination you want to patch signals into:
            </p>
          </div>
          <Tabs
            selectedKey={destCategory}
            onSelectionChange={(key) => setDestCategory(key as DestCategory)}
            variant="solid"
            color="primary"
            size="sm"
          >
            <Tab key="PhysicalDac" title="⚡ Physical CV Out Jacks" />
            <Tab key="AppInput" title="🎛️ App Software Inputs" />
            <Tab key="AppFader" title="🎚️ Fader Position Modulation" />
          </Tabs>
        </div>

        {/* Matrix Grid */}
        <div className="overflow-x-auto pt-2">
          <div className="min-w-[750px]">
            {/* Header row */}
            <div className="grid grid-cols-[180px_repeat(16,minmax(32px,1fr))] gap-1 border-b border-neutral-800 pb-2 text-center text-[11px] font-semibold text-neutral-300">
              <div className="pl-2 text-left">Signal Source \ Target</div>
              {Array.from({ length: 16 }, (_, i) => (
                <div key={i} title={getColHeaderLabel(destCategory, i)}>
                  {getColHeaderLabel(destCategory, i)}
                </div>
              ))}
            </div>

            {/* Source Rows */}
            {Array.from({ length: 16 }, (_, srcChan) => {
              const src: routing.RouteSource = {
                tag: "AppOutput",
                value: { channel: srcChan },
              };
              return (
                <div
                  key={srcChan}
                  className="grid grid-cols-[180px_repeat(16,minmax(32px,1fr))] items-center gap-1 rounded-sm py-1 hover:bg-neutral-800/40"
                >
                  <div
                    className="truncate pl-2 text-xs font-medium text-neutral-300"
                    title={getAppName(srcChan)}
                  >
                    {srcChan + 1}. {getAppName(srcChan)}
                  </div>
                  {Array.from({ length: 16 }, (_, destChan) => {
                    const dest = createDestForCategory(destCategory, destChan);
                    const activeRoute = routeList.find(
                      (r) =>
                        r &&
                        r.enabled &&
                        isSameSource(r.source, src) &&
                        isSameDest(r.destination, dest),
                    );

                    return (
                      <button
                        key={destChan}
                        onClick={() => handleCellClick(src, dest)}
                        className={`flex h-7 w-7 items-center justify-center rounded border text-[10px] font-bold transition-all ${
                          activeRoute
                            ? "border-cyan-400 bg-cyan-500/30 text-cyan-200 shadow-[0_0_8px_rgba(6,182,212,0.4)]"
                            : "border-neutral-800 bg-neutral-950/40 text-neutral-600 hover:border-neutral-600 hover:text-neutral-300"
                        }`}
                        title={
                          activeRoute
                            ? `Route Active: ${formatSourceLabel(src)} → ${formatDestLabel(dest)} (${activeRoute.mode.tag})`
                            : `Connect ${formatSourceLabel(src)} to ${formatDestLabel(dest)}`
                        }
                      >
                        {activeRoute ? "●" : "+"}
                      </button>
                    );
                  })}
                </div>
              );
            })}
          </div>
        </div>
      </div>

      {/* Active Routes List Summary */}
      <div className="rounded-xl border border-neutral-800 bg-neutral-900/60 p-4">
        <div className="pb-3">
          <h3 className="text-lg font-semibold text-white">
            Active Connections ({activeCount} / 32)
          </h3>
        </div>
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2 lg:grid-cols-3">
          {routeList.map((r, idx) => {
            if (!r) return null;
            return (
              <div
                key={idx}
                className="flex items-center justify-between rounded-md border border-neutral-800 bg-neutral-950 p-3 transition-all hover:border-neutral-700"
              >
                <div className="flex min-w-0 flex-col gap-1 pr-2">
                  <div className="flex items-center gap-2 truncate text-xs font-semibold text-cyan-400">
                    <span>{formatSourceLabel(r.source)}</span>
                    <span>→</span>
                    <span>{formatDestLabel(r.destination)}</span>
                  </div>
                  <div className="text-[11px] text-neutral-400">
                    Mode:{" "}
                    <span className="font-medium text-white">{r.mode.tag}</span>{" "}
                    | Att: {r.attenuation_percent}% | Offset: {r.offset}
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <Switch
                    size="sm"
                    isSelected={r.enabled}
                    onValueChange={(val) => {
                      const newRoutes = [...routeList];
                      newRoutes[idx] = { ...r, enabled: val };
                      setRouting({
                        routes: newRoutes as unknown,
                      } as routing.RoutingConfig);
                    }}
                  />
                  <Button
                    size="sm"
                    isIconOnly
                    color="danger"
                    variant="light"
                    onPress={() => handleDeleteRoute(idx)}
                  >
                    ×
                  </Button>
                </div>
              </div>
            );
          })}
          {activeCount === 0 && (
            <div className="col-span-full py-8 text-center text-sm text-neutral-500">
              No active routing connections. Select a target category above and
              click any matrix node to patch signals.
            </div>
          )}
        </div>
      </div>

      {/* Route Edit Modal */}
      {editingRoute && (
        <Modal
          isOpen={selectedRouteIdx !== null}
          onClose={() => setSelectedRouteIdx(null)}
          backdrop="blur"
          radius="sm"
        >
          <ModalContent className="border border-neutral-800 bg-neutral-900 text-white">
            <ModalHeader className="flex flex-col gap-1">
              <h3>Configure Route Connection</h3>
              <p className="text-xs text-neutral-400">
                {formatSourceLabel(editingRoute.source)} →{" "}
                {formatDestLabel(editingRoute.destination)}
              </p>
            </ModalHeader>
            <ModalBody className="flex flex-col gap-4">
              <div>
                <label className="mb-1 block text-xs text-neutral-300">
                  Combine Mode
                </label>
                <Select
                  selectedKeys={[editingRoute.mode.tag]}
                  onSelectionChange={(keys) => {
                    const tag = Array.from(
                      keys,
                    )[0] as routing.CombineMode["tag"];
                    if (tag)
                      setEditingRoute({
                        ...editingRoute,
                        mode: { tag } as routing.CombineMode,
                      });
                  }}
                  className="rounded-md border border-neutral-800 bg-neutral-950"
                >
                  {COMBINE_MODES.map((m) => (
                    <SelectItem key={m.modeTag}>{m.label}</SelectItem>
                  ))}
                </Select>
              </div>

              <div>
                <Slider
                  label="Gain / Attenuation (%)"
                  step={5}
                  minValue={-200}
                  maxValue={200}
                  value={editingRoute.attenuation_percent}
                  onChange={(val) =>
                    setEditingRoute({
                      ...editingRoute,
                      attenuation_percent: val as number,
                    })
                  }
                  className="max-w-md"
                />
              </div>

              <div>
                <Slider
                  label="Bipolar Offset (DAC counts)"
                  step={16}
                  minValue={-2048}
                  maxValue={2047}
                  value={editingRoute.offset}
                  onChange={(val) =>
                    setEditingRoute({
                      ...editingRoute,
                      offset: val as number,
                    })
                  }
                  className="max-w-md"
                />
              </div>

              <div className="flex items-center justify-between">
                <span className="text-xs text-neutral-300">Enable Route</span>
                <Switch
                  isSelected={editingRoute.enabled}
                  onValueChange={(val) =>
                    setEditingRoute({ ...editingRoute, enabled: val })
                  }
                />
              </div>
            </ModalBody>
            <ModalFooter>
              <Button
                color="danger"
                variant="flat"
                onPress={() => setSelectedRouteIdx(null)}
              >
                Cancel
              </Button>
              <Button color="primary" onPress={handleSaveRoute}>
                Save Route
              </Button>
            </ModalFooter>
          </ModalContent>
        </Modal>
      )}
    </div>
  );
};
