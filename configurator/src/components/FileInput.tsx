import { ChangeEvent, useEffect, useRef } from "react";
import { button } from "@heroui/theme";
import classNames from "classnames";

interface Props {
  onLoadFile: (file: File) => void;
  file?: File;
  buttonText?: string;
}

export const FileInput = ({ buttonText, file, onLoadFile }: Props) => {
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!file && inputRef.current) {
      inputRef.current.value = "";
    }
  }, [file]);

  const handleFileChange = (e: ChangeEvent<HTMLInputElement>) => {
    const selectedFile = e.target.files?.[0];
    if (selectedFile) {
      onLoadFile(selectedFile);
    }
  };

  return (
    <div className="flex items-center gap-2">
      <input
        ref={inputRef}
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
      <span className="flex-1 text-gray-700">
        {file ? file.name : "No file chosen"}
      </span>
    </div>
  );
};
