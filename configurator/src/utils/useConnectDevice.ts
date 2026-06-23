import { useState } from "react";
import { compare, major, minor } from "semver";
import { addToast } from "@heroui/toast";

import { useStore } from "../store";
import { getDeviceVersion } from "./usb-protocol";
import { useLatestFirmwareVersion } from "../useLatestFirmwareVersion";

const FADERPUNK_VENDOR_ID = 0xf569;
const FADERPUNK_PRODUCT_ID = 0x1;

// gh-pages hosts each firmware line under its own folder (/1.9/, /1.10/, …).
export function versionPath(version: string): string {
  return `/${major(version)}.${minor(version)}/`;
}

type UpdateAvailable = {
  currentVersion: string;
  latestVersion: string;
  configuratorPath: string;
};

/**
 * Drives the "Connect Device" action from the simulator. Reads the connected
 * device's firmware version and either connects in place (when this build
 * already matches the device's version) or redirects to the matching versioned
 * configurator deployment. Ports the logic that used to live in the standalone
 * landing page.
 */
export function useConnectDevice() {
  const autoConnect = useStore((s) => s.autoConnect);
  const latestVersion = useLatestFirmwareVersion();
  const [connecting, setConnecting] = useState(false);
  const [updateAvailable, setUpdateAvailable] =
    useState<UpdateAvailable | null>(null);
  const webUsbSupported = typeof navigator !== "undefined" && !!navigator.usb;

  // Connect in place when the target deployment is the one we're already
  // running (BASE_URL is "/1.10/" on a versioned build, "/" at the root);
  // otherwise hand off to the matching versioned deployment.
  async function goToConfigurator(configuratorPath: string) {
    if (configuratorPath === import.meta.env.BASE_URL) {
      const ok = await autoConnect();
      if (!ok) {
        setConnecting(false);
        addToast({
          title: "Couldn't connect",
          description:
            "The device was found but didn't respond. Unplug it, plug it back in and try again.",
          color: "danger",
        });
      }
      // On success the store sets usbDevice, the simulator banner unmounts and
      // this hook goes with it — no need to reset `connecting`.
      return;
    }
    // Navigating away; keep the spinner until the new page takes over.
    window.location.href = configuratorPath;
  }

  async function connect() {
    setConnecting(true);
    setUpdateAvailable(null);

    try {
      if (!navigator.usb) {
        throw new Error(
          "WebUSB is not supported in this browser. Please use Chrome, Edge, or another Chromium-based browser.",
        );
      }

      const device = await navigator.usb.requestDevice({
        filters: [
          {
            classCode: 0xff,
            vendorId: FADERPUNK_VENDOR_ID,
            productId: FADERPUNK_PRODUCT_ID,
          },
        ],
      });

      await device.open();
      const deviceVersion = getDeviceVersion(device);
      await device.close();

      let configuratorPath: string;
      if (compare(deviceVersion, latestVersion) > 0) {
        // Newer than the latest stable release → beta deployment.
        configuratorPath = "/beta/";
      } else if (compare(deviceVersion, "1.7.0") < 0) {
        // Older than the oldest hosted line → legacy deployment.
        configuratorPath = "/1.6/";
      } else {
        configuratorPath = versionPath(deviceVersion);
      }

      // Outdated firmware → let the user choose update vs. continue.
      if (compare(deviceVersion, latestVersion) < 0) {
        setUpdateAvailable({
          currentVersion: deviceVersion,
          latestVersion,
          configuratorPath,
        });
        setConnecting(false);
        return;
      }

      await goToConfigurator(configuratorPath);
    } catch (error) {
      const err = error as Error;
      console.error("Connection error:", err);
      setConnecting(false);

      if (err.name === "NotFoundError") {
        addToast({
          title: "No device selected",
          description: "Pick your Faderpunk in the browser dialog to connect.",
          color: "warning",
        });
      } else {
        addToast({
          title: "Couldn't connect",
          description: err.message || "Unknown error",
          color: "danger",
        });
      }
    }
  }

  function dismissUpdate() {
    setUpdateAvailable(null);
  }

  function updateFirmware() {
    if (!updateAvailable) return;
    // Skip auto-connect on the update page so it doesn't grab the device.
    sessionStorage.setItem("fp-skip-autoconnect", "1");
    window.location.href =
      versionPath(updateAvailable.latestVersion) + "#/update";
  }

  async function continueAnyway() {
    if (!updateAvailable) return;
    const { configuratorPath } = updateAvailable;
    setUpdateAvailable(null);
    setConnecting(true);
    await goToConfigurator(configuratorPath);
  }

  return {
    connect,
    connecting,
    webUsbSupported,
    updateAvailable,
    dismissUpdate,
    updateFirmware,
    continueAnyway,
  };
}
