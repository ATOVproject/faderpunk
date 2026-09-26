import type { ChangeEvent } from "react";
import { button } from "@heroui/theme";
import classNames from "classnames";

interface Props {
  onLoadFile: (file: File) => void;
  buttonText?: string;
}

export const FileInput = ({ buttonText, onLoadFile }: Props) => {
  const handleFileChange = (e: ChangeEvent<HTMLInputElement>) => {
    const selectedFile = e.target.files?.[0];
    if (selectedFile) {
      onLoadFile(selectedFile);
    }
    // Reset so choosing the same file again still fires onChange
    e.target.value = "";
  };

  return (
    <div className="flex items-center gap-2">
      <input
        type="file"
        onChange={handleFileChange}
        className="hidden"
        id="file-upload"
        accept=".json,application/json"
      />
      <label
        htmlFor="file-upload"
        className={classNames(
          button({
            color: "primary",
            radius: "sm",
          }),
          "px-8 py-2.5 text-sm font-semibold",
        )}
      >
        {buttonText ? buttonText : "Choose file"}
      </label>
    </div>
  );
};
