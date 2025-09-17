import type { Param, Color, AppIcon } from "@atov/fp-config";

export interface App {
  appId: number;
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
  id: string;
  start: number;
  end: number;
}

export interface EmptySlot {
  id: string;
  slotNumber: number;
}

export type AppSlot = AppInLayout | EmptySlot;

export type AppLayout = AppSlot[];
