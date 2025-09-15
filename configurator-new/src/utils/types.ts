import type { Param, Color, AppIcon } from "@atov/fp-config";

export interface App {
  id: number;
  channels: bigint;
  color: Color["tag"];
  name: string;
  description: string;
  icon: AppIcon["tag"];
  paramCount: bigint;
  params: Param[];
}

export type AllApps = Map<number, App>;

export interface AppInLayout extends App {
  start: number;
  end: number;
}

export interface EmptySlot {
  slotNumber: number;
}

export type AppLayout = (AppInLayout | EmptySlot)[];
