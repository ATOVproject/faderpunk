import type {
  AppIcon,
  ConfigMsgOut,
  Color,
  Param,
  Value,
} from "@atov/fp-config";

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

export interface AppFile {
  appId: number | null;
  layoutId: number;
  params: Value[] | null;
  startChannel: number;
}

export interface LayoutFile {
  version: number;
  layout: AppFile[];
}

export type AppLayout = AppSlot[];

export type AllColors = Color["tag"] | "Black";

export enum ModalMode {
  EditLayout,
  AddApp,
  RecallLayout,
}

export interface ModalConfig {
  isOpen: boolean;
  mode: ModalMode;
  appToAdd?: number;
  recallLayout?: AppLayout;
  recallParams?: ParamValues;
}

export type AppParams = Extract<ConfigMsgOut, { tag: "AppState" }>;

export type ParamValues = Map<number, Value[]>;

export type RecoveredLayout = { layout: AppLayout; params: ParamValues };
