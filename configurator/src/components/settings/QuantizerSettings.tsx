import type { Key, Note } from "@atov/fp-config";
import { Select, SelectItem } from "@heroui/select";

import { selectProps } from "../input/defaultProps";
import type { Inputs } from "../SettingsTab";
import { useFormContext } from "react-hook-form";

interface QuantizerKeyItem {
  key: Key["tag"];
  value: string;
}

interface QuantizerTonicItem {
  key: Note["tag"];
  value: string;
}

const keyItems: QuantizerKeyItem[] = [
  { key: "Chromatic", value: "Chromatic" },
  { key: "Ionian", value: "Ionian" },
  { key: "Dorian", value: "Dorian" },
  { key: "Phrygian", value: "Phrygian" },
  { key: "Lydian", value: "Lydian" },
  { key: "Mixolydian", value: "Mixolydian" },
  { key: "Aeolian", value: "Aeolian" },
  { key: "Locrian", value: "Locrian" },
  { key: "BluesMaj", value: "Blues Major" },
  { key: "BluesMin", value: "Blues Minor" },
  { key: "PentatonicMaj", value: "Pentatonic Major" },
  { key: "PentatonicMin", value: "Pentatonic Minor" },
  { key: "Folk", value: "Folk" },
  { key: "Japanese", value: "Japanese" },
  { key: "Gamelan", value: "Gamelan" },
  { key: "HungarianMin", value: "Hungarian Minor" },
];

const tonicItems: QuantizerTonicItem[] = [
  "C",
  "CSharp",
  "D",
  "DSharp",
  "E",
  "F",
  "FSharp",
  "G",
  "GSharp",
  "A",
  "ASharp",
  "B",
].map((note) => ({
  key: note as Note["tag"],
  value: note.replace("Sharp", "♯"),
}));

export const QuantizerSettings = () => {
  const { register } = useFormContext<Inputs>();

  return (
    <div className="mb-12">
      <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
        Quantizer
      </h2>
      <div className="grid grid-cols-4 gap-x-16 gap-y-8 px-4">
        <Select
          {...register("quantizerKey")}
          {...selectProps}
          label="Scale"
          items={keyItems}
          placeholder="Scale"
        >
          {(item) => <SelectItem>{item.value}</SelectItem>}
        </Select>
        <Select
          {...register("quantizerTonic")}
          {...selectProps}
          label="Tonic"
          items={tonicItems}
          placeholder="Tonic"
        >
          {(item) => <SelectItem>{item.value}</SelectItem>}
        </Select>
      </div>
    </div>
  );
};
