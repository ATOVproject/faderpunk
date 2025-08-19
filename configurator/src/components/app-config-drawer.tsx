import { Button } from "@heroui/button";
import {
  Drawer,
  DrawerContent,
  DrawerHeader,
  DrawerBody,
} from "@heroui/drawer";
import { Form } from "@heroui/form";
import { useEffect, useState } from "react";

import {
  I32ParamInput,
  F32ParamInput,
  BoolParamInput,
  EnumParamInput,
  CurveParamInput,
  WaveformParamInput,
  ColorParamInput,
} from "@/components/param-inputs";
import { getAppParams, setAppParams } from "@/utils/config";

interface AppConfigDrawerProps {
  isOpen: boolean;
  onClose: () => void;
  selectedApp: { appId: string; startChannel: number } | null;
  appConfig: {
    appId: string;
    channels: string;
    name: string;
    description: string;
    paramCount: string;
    params: any[];
  } | null;
  usbDevice: USBDevice;
}

export function AppConfigDrawer({
  isOpen,
  onClose,
  selectedApp,
  appConfig,
  usbDevice,
}: AppConfigDrawerProps) {
  const [currentAppParams, setCurrentAppParams] = useState<any>(null);
  const [paramValues, setParamValues] = useState<any[]>([]);

  const fetchParams = async () => {
    if (!selectedApp) {
      return;
    }
    const response = await getAppParams(
      usbDevice,
      selectedApp.startChannel.toString(),
    );

    if (response && response.tag === "AppState") {
      setCurrentAppParams(response);
      setParamValues(response.value[1]);
    }
  };

  useEffect(() => {
    if (isOpen) {
      fetchParams();
    }
  }, [selectedApp, isOpen]);

  const handleParamChange = (index: number, value: any) => {
    setParamValues((prev) => {
      const newValues = [...prev];

      newValues[index] = value;

      return newValues;
    });
  };

  const handleSaveParams = async () => {
    if (!selectedApp || !usbDevice) {
      return;
    }

    await setAppParams(usbDevice, selectedApp.startChannel, paramValues);
    onClose();
  };

  const renderParamInput = (param: any, value: any, index: number) => {
    if (!param || param.tag === "None") {
      return null;
    }

    const commonProps = {
      label: param.value?.name || `Parameter ${index + 1}`,
    };

    switch (param.tag) {
      case "i32":
        return (
          <I32ParamInput
            key={`param-${index}`}
            {...commonProps}
            max={param.value?.max || 2147483647}
            min={param.value?.min || -2147483648}
            value={
              typeof value?.value === "number"
                ? value.value
                : typeof value === "number"
                  ? value
                  : 0
            }
            onChange={(newValue) =>
              handleParamChange(index, { tag: "i32", value: newValue })
            }
          />
        );
      case "Float":
        return (
          <F32ParamInput
            key={`param-${index}`}
            {...commonProps}
            value={
              typeof value?.value === "number"
                ? value.value
                : typeof value === "number"
                  ? value
                  : 0.0
            }
            onChange={(newValue) =>
              handleParamChange(index, { tag: "f32", value: newValue })
            }
          />
        );
      case "Bool":
        return (
          <BoolParamInput
            key={`param-${index}`}
            {...commonProps}
            isSelected={value?.value || false}
            onValueChange={(newValue) =>
              handleParamChange(index, { tag: "bool", value: newValue })
            }
          />
        );
      case "Enum":
        return (
          <EnumParamInput
            key={`param-${index}`}
            {...commonProps}
            selectedKeys={
              value?.value !== undefined ? [value.value.toString()] : []
            }
            variants={param.value?.variants || []}
            onSelectionChange={(keys) => {
              const selectedValue = Array.from(keys)[0];

              if (selectedValue !== undefined) {
                handleParamChange(index, {
                  tag: "Enum",
                  value: parseInt(selectedValue.toString()),
                });
              }
            }}
          />
        );
      case "Curve":
        return (
          <CurveParamInput
            key={`param-${index}`}
            {...commonProps}
            selectedKeys={
              value?.value?.tag
                ? [value.value.tag]
                : value?.tag
                  ? [value.tag]
                  : ["Linear"]
            }
            variants={
              param.value?.variants?.map((v: any) => v.tag || v) || [
                "Linear",
                "Exponential",
                "Logarithmic",
              ]
            }
            onSelectionChange={(keys) => {
              const selectedValue = Array.from(keys)[0];

              if (selectedValue) {
                handleParamChange(index, {
                  tag: "Curve",
                  value: { tag: selectedValue },
                });
              }
            }}
          />
        );
      case "Waveform":
        return (
          <WaveformParamInput
            key={`param-${index}`}
            {...commonProps}
            selectedKeys={value?.value?.tag ? [value.value.tag] : ["Triangle"]}
            variants={
              param.value?.variants?.map((v: any) => v.tag || v) || [
                "Triangle",
                "Saw",
                "Rect",
                "Sine",
              ]
            }
            onSelectionChange={(keys) => {
              const selectedValue = Array.from(keys)[0];

              if (selectedValue) {
                handleParamChange(index, {
                  tag: "Waveform",
                  value: { tag: selectedValue },
                });
              }
            }}
          />
        );
      case "Color":
        return (
          <ColorParamInput
            key={`param-${index}`}
            {...commonProps}
            selectedKeys={value?.value?.tag ? [value.value.tag] : ["White"]}
            variants={
              param.value?.variants?.map((v: any) => v.tag || v) || [
                "White",
                "Red",
                "Blue",
                "Yellow",
                "Purple",
              ]
            }
            onSelectionChange={(keys) => {
              const selectedValue = Array.from(keys)[0];

              if (selectedValue) {
                handleParamChange(index, {
                  tag: "Color",
                  value: { tag: selectedValue },
                });
              }
            }}
          />
        );
      default:
        return null;
    }
  };

  return (
    <Drawer isOpen={isOpen} placement="right" size="md" onClose={onClose}>
      <DrawerContent>
        <DrawerHeader>
          Configure {appConfig?.name || `App ${selectedApp?.appId}`} (Channel{" "}
          {selectedApp ? selectedApp.startChannel + 1 : ""})
        </DrawerHeader>
        <DrawerBody>
          <Form className="space-y-4">
            {appConfig?.params && paramValues.length > 0 ? (
              appConfig.params
                .map((param, index) =>
                  renderParamInput(param, paramValues[index], index),
                )
                .filter(Boolean)
            ) : (
              <div className="text-center text-gray-500 py-4">
                {currentAppParams
                  ? "No parameters available"
                  : "Loading parameters..."}
              </div>
            )}
            <div className="flex gap-2 pt-4">
              {paramValues.length ? (
                <Button
                  className="flex-1"
                  color="primary"
                  onPress={handleSaveParams}
                >
                  Save Parameters
                </Button>
              ) : null}
              <Button className="flex-1" variant="flat" onPress={onClose}>
                Cancel
              </Button>
            </div>
          </Form>
        </DrawerBody>
      </DrawerContent>
    </Drawer>
  );
}
