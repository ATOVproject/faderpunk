import { Input, Textarea } from "@heroui/input";

import { GlobalConfig } from "@atov/fp-config";
import {
  AllApps,
  LayoutFile,
  ModalMode,
  ParamValues,
  RecoveredLayout,
  type AppLayout,
} from "../utils/types";
import { ButtonPrimary, ButtonSecondary } from "./Button";
import { useModalContext } from "../contexts/ModalContext";
import {
  deserializeLayout,
  recoverLayout,
  saveLayout,
  serializeLayout,
} from "../utils/config";
import { useStore } from "../store";
import { FileInput } from "./FileInput";
import { inputProps } from "./input/defaultProps";
import { useCallback, useState } from "react";

const saveFile = (
  layout: AppLayout,
  params: ParamValues,
  config: GlobalConfig,
  filename = "faderpunk-setup.json",
  description?: string,
) => {
  const layoutFile = saveLayout(layout, params, config, description);
  const jsonString = serializeLayout(layoutFile);
  const blob = new Blob([jsonString], { type: "application/json" });
  const url = URL.createObjectURL(blob);

  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();

  URL.revokeObjectURL(url);
};

const loadFile = (file: File, apps: AllApps): Promise<RecoveredLayout> => {
  if (!file) {
    return Promise.reject();
  }
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = (ev: ProgressEvent<FileReader>) => {
      try {
        const layoutFile: LayoutFile = deserializeLayout(
          ev.target?.result as string,
        );
        const recoveredLayout = recoverLayout(layoutFile, apps);
        resolve(recoveredLayout);
      } catch (error) {
        reject(error);
      }
    };
    reader.readAsText(file);
  });
};

export const SaveLoadSetup = () => {
  const { setModalConfig } = useModalContext();
  const { apps, params, layout, config } = useStore();
  const [filename, setFilename] = useState<string>("faderpunk-setup");
  const [description, setDescription] = useState<string>("");
  const [loadedFile, setLoadedFile] = useState<File | undefined>();
  const [error, setError] = useState<string | undefined>();

  const handleLoadFile = useCallback((file: File) => {
    setError(undefined);
    setLoadedFile(file);
  }, []);

  if (!layout || !params || !apps || !config) {
    return null;
  }

  const showSave = layout && layout.some((slot) => !!slot.app);

  return (
    <>
      {showSave ? (
        <>
          <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
            Save Setup (Layout, Params, Config)
          </h2>
          <div className="mb-12 px-4">
            <Input
              {...inputProps}
              endContent={
                <div className="pointer-events-none flex items-center">
                  <span className="text-default-400 text-small">.json</span>
                </div>
              }
              label="File name"
              defaultValue={filename}
              onChange={(e) => {
                setFilename(e.target.value);
              }}
              className="mb-4 max-w-2xs"
            />
            <div className="my-4 max-w-md">
              <details className="group">
                <summary className="cursor-pointer text-sm font-medium">
                  Add description
                </summary>
                <div className="mt-2">
                  <Textarea
                    classNames={{ label: "font-medium", input: "py-2" }}
                    disableAnimation
                    radius="sm"
                    labelPlacement="outside-top"
                    label="Description"
                    placeholder="Enter Setup description"
                    value={description}
                    onValueChange={setDescription}
                  />
                </div>
              </details>
            </div>
            <ButtonPrimary
              type="button"
              onPress={() =>
                saveFile(layout, params, config, filename, description)
              }
            >
              Save current Setup
            </ButtonPrimary>
          </div>
        </>
      ) : null}
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
        Load Setup
      </h2>
      <div className="mb-12 px-4">
        <FileInput
          buttonText="Choose Setup file"
          file={loadedFile}
          onLoadFile={handleLoadFile}
        />
        {loadedFile ? (
          <>
            <ButtonPrimary
              type="button"
              className="mt-4"
              onPress={async () => {
                setError(undefined);
                try {
                  const {
                    layout,
                    params,
                    config: loadedConfig,
                    description: loadedDescription,
                  } = await loadFile(loadedFile, apps);

                  setModalConfig({
                    isOpen: true,
                    mode: ModalMode.RecallLayout,
                    recallLayout: layout,
                    recallParams: params,
                    recallConfig: loadedConfig,
                    recallDescription: loadedDescription,
                  });
                  setLoadedFile(undefined);
                } catch {
                  setError("Could not read config file");
                }
              }}
            >
              Load
            </ButtonPrimary>
            <ButtonSecondary
              type="button"
              onPress={() => {
                setError(undefined);
                setLoadedFile(undefined);
              }}
            >
              Cancel
            </ButtonSecondary>
          </>
        ) : null}
        {error && <div className="text-danger mt-4">{error}</div>}
      </div>
    </>
  );
};
