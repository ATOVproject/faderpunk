import { Input } from "@heroui/input";

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
  filename = "faderpunk-layout.json",
) => {
  const layoutFile = saveLayout(layout, params);
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

export const SaveLoadLayout = () => {
  const { setModalConfig } = useModalContext();
  const { apps, params, layout } = useStore();
  const [filename, setFilename] = useState<string>("faderpunk-layout");
  const [loadedFile, setLoadedFile] = useState<File | undefined>();
  const [error, setError] = useState<string | undefined>();

  const handleLoadFile = useCallback((file: File) => {
    setError(undefined);
    setLoadedFile(file);
  }, []);

  if (!layout || !params || !apps) {
    return null;
  }

  const showSave = layout && layout.some((slot) => !!slot.app);

  return (
    <>
      {showSave ? (
        <>
          <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
            Save App Layout &amp; Params
          </h2>
          <div className="mb-12">
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
            <ButtonPrimary onPress={() => saveFile(layout, params, filename)}>
              Save current layout
            </ButtonPrimary>
          </div>
        </>
      ) : null}
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
        Load App Layout &amp; Params
      </h2>
      <div>
        <FileInput file={loadedFile} onLoadFile={handleLoadFile} />
        {loadedFile ? (
          <>
            <ButtonPrimary
              className="mt-4"
              onPress={async () => {
                setError(undefined);
                try {
                  const { layout, params } = await loadFile(loadedFile, apps);
                  setModalConfig({
                    isOpen: true,
                    mode: ModalMode.RecallLayout,
                    recallLayout: layout,
                    recallParams: params,
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
