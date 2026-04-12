import type { ReactNode } from "react";
import type {
  FieldPath,
  FieldValues,
  UseControllerProps,
} from "react-hook-form";
import { Controller } from "react-hook-form";
import {
  Select,
  SelectItem as HeroSelectItem,
  type SelectProps,
} from "@heroui/select";
import { Switch, type SwitchProps } from "@heroui/switch";
import { Checkbox, type CheckboxProps } from "@heroui/checkbox";
import { Slider, type SliderProps } from "@heroui/slider";

import { selectProps as defaultSelectProps } from "../input/defaultProps";

// --- Controlled Select ---

type SelectItem = { key: string; value: string };

interface ControlledSelectProps<
  TFieldValues extends FieldValues,
  TName extends FieldPath<TFieldValues>,
> extends UseControllerProps<TFieldValues, TName> {
  items: SelectItem[];
  children: (item: SelectItem) => ReturnType<typeof HeroSelectItem>;
  selectProps?: Partial<SelectProps>;
  label?: ReactNode;
  placeholder?: string;
  isDisabled?: boolean;
}

export const ControlledSelect = <
  TFieldValues extends FieldValues,
  TName extends FieldPath<TFieldValues>,
>({
  items,
  children,
  selectProps,
  label,
  placeholder,
  isDisabled,
  ...controllerProps
}: ControlledSelectProps<TFieldValues, TName>) => (
  <Controller
    {...controllerProps}
    render={({ field }) => (
      <Select
        selectedKeys={[String(field.value)]}
        onSelectionChange={(value) => field.onChange(value.currentKey)}
        isDisabled={isDisabled}
        {...defaultSelectProps}
        {...selectProps}
        label={label}
        items={items}
        placeholder={placeholder}
      >
        {children}
      </Select>
    )}
  />
);

// --- Controlled Switch ---

interface ControlledSwitchProps<
  TFieldValues extends FieldValues,
  TName extends FieldPath<TFieldValues>,
> extends UseControllerProps<TFieldValues, TName> {
  children: ReactNode;
  switchProps?: Partial<SwitchProps>;
}

export const ControlledSwitch = <
  TFieldValues extends FieldValues,
  TName extends FieldPath<TFieldValues>,
>({
  children,
  switchProps,
  ...controllerProps
}: ControlledSwitchProps<TFieldValues, TName>) => (
  <Controller
    {...controllerProps}
    render={({ field }) => (
      <Switch
        isSelected={!!field.value}
        onValueChange={field.onChange}
        {...switchProps}
      >
        {children}
      </Switch>
    )}
  />
);

// --- Controlled Checkbox ---

interface ControlledCheckboxProps<
  TFieldValues extends FieldValues,
  TName extends FieldPath<TFieldValues>,
> extends UseControllerProps<TFieldValues, TName> {
  children: ReactNode;
  checkboxProps?: Partial<CheckboxProps>;
}

export const ControlledCheckbox = <
  TFieldValues extends FieldValues,
  TName extends FieldPath<TFieldValues>,
>({
  children,
  checkboxProps,
  ...controllerProps
}: ControlledCheckboxProps<TFieldValues, TName>) => (
  <Controller
    {...controllerProps}
    render={({ field }) => (
      <Checkbox
        isSelected={!!field.value}
        onValueChange={field.onChange}
        {...checkboxProps}
      >
        {children}
      </Checkbox>
    )}
  />
);

// --- Controlled Slider ---

interface ControlledSliderProps<
  TFieldValues extends FieldValues,
  TName extends FieldPath<TFieldValues>,
> extends UseControllerProps<TFieldValues, TName> {
  sliderProps?: Partial<SliderProps>;
  label?: ReactNode;
  minValue?: number;
  maxValue?: number;
}

export const ControlledSlider = <
  TFieldValues extends FieldValues,
  TName extends FieldPath<TFieldValues>,
>({
  label,
  minValue,
  maxValue,
  sliderProps,
  ...controllerProps
}: ControlledSliderProps<TFieldValues, TName>) => (
  <Controller
    {...controllerProps}
    render={({ field }) => (
      <Slider
        value={field.value as number}
        onChange={(value) => {
          if (!Array.isArray(value)) field.onChange(value);
        }}
        label={label}
        minValue={minValue}
        maxValue={maxValue}
        {...sliderProps}
      />
    )}
  />
);
