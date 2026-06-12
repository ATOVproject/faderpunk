// FIRMWARE_LATEST_VERSION is injected at build time from release-please manifest.
// See vite.config.mjs for implementation and vite-env.d.ts for type declaration.
export const FIRMWARE_LATEST_VERSION = __FIRMWARE_LATEST_VERSION__;

// True for the dedicated /simulator deployment, which boots straight into
// simulator mode and has no device connect page (see release.yml).
export const IS_SIMULATOR_BUILD = import.meta.env.VITE_SIMULATOR === "true";
