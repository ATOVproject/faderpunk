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

export interface AppSlot {
  id: number;
  app: App | null;
  startChannel: number;
}

export type AppLayout = AppSlot[];
