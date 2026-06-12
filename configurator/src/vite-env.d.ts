/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Set to "true" for the dedicated /simulator deployment build. */
  readonly VITE_SIMULATOR?: string;
}

/**
 * Build-time constant injected by Vite.
 * Contains the firmware version from release-please manifest.
 */
declare const __FIRMWARE_LATEST_VERSION__: string;
