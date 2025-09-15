import { type Color } from "@atov/fp-config";
import { horizontalListSortingStrategy } from "@dnd-kit/sortable";

import { List } from "./List.tsx";
import { Sortable } from "./Sortable";
import { COLORS_CLASSES, WIDTHS_CLASSES } from "../class-helpers.ts";

const WIDTHS = [2, 1, 8, 1, 1, 2];
const COLORS = ["Red", "Blue", "Yellow", "Pink", "Green", "Violet"];

export const VariableWidths = () => {
  return (
    <Sortable
      Container={(props: any) => <List horizontal {...props} />}
      itemCount={6}
      strategy={horizontalListSortingStrategy}
      // TODO: We need wrapperClasses for tailwind
      wrapperClasses={({ id }) =>
        `${WIDTHS_CLASSES[WIDTHS[Number(id)]]} ${COLORS_CLASSES[COLORS[Number(id)]]}`
      }
    />
  );
};
