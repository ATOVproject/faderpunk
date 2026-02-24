import { useEffect, useState } from "react";

const FADERPUNK_VENDOR_ID = 0xf569;
const FADERPUNK_PRODUCT_ID = 0x1;
const FIRMWARE_LATEST_VERSION = __FIRMWARE_LATEST_VERSION__;
const VERSION_PATH =
  "/" + FIRMWARE_LATEST_VERSION.split(".").slice(0, 2).join(".") + "/";

type State =
  | { status: "idle" }
  | { status: "connecting" }
  | { status: "redirecting" }
  | {
      status: "update-available";
      currentVersion: string;
      configuratorPath: string;
    }
  | { status: "error"; message: string };

function parseVersion(versionString: string) {
  const parts = versionString.split(".").map(Number);
  return {
    major: parts[0] || 0,
    minor: parts[1] || 0,
    patch: parts[2] || 0,
  };
}

function compareVersions(v1String: string, v2String: string) {
  const v1 = parseVersion(v1String);
  const v2 = parseVersion(v2String);

  if (v1.major !== v2.major) return v1.major - v2.major;
  if (v1.minor !== v2.minor) return v1.minor - v2.minor;
  return v1.patch - v2.patch;
}

export default function App() {
  const [state, setState] = useState<State>({ status: "idle" });
  const webUsbSupported = !!navigator.usb;

  // Redirect hash routes to the versioned deployment
  useEffect(() => {
    if (window.location.hash) {
      window.location.replace(VERSION_PATH + window.location.hash);
    }
  }, []);

  async function connectAndRedirect() {
    setState({ status: "connecting" });

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

      const deviceVersionString = `${device.deviceVersionMajor}.${device.deviceVersionMinor}.${device.deviceVersionSubminor || 0}`;
      const versionPath = `${device.deviceVersionMajor}.${device.deviceVersionMinor}`;

      await device.close();

      // Determine target path for the matching configurator
      let configuratorPath: string;

      if (compareVersions(deviceVersionString, FIRMWARE_LATEST_VERSION) > 0) {
        // Device version is newer than latest stable → beta
        configuratorPath = "/beta/";
      } else if (compareVersions(deviceVersionString, "1.7.0") < 0) {
        // Device version is older than 1.7.0 → legacy
        configuratorPath = "/1.6/";
      } else {
        configuratorPath = `/${versionPath}/`;
      }

      // Firmware is outdated → show update choice
      if (compareVersions(deviceVersionString, FIRMWARE_LATEST_VERSION) < 0) {
        setState({
          status: "update-available",
          currentVersion: deviceVersionString,
          configuratorPath,
        });
        return;
      }

      // Firmware is current or newer → auto-redirect
      setState({ status: "redirecting" });
      setTimeout(() => {
        window.location.href = configuratorPath;
      }, 500);
    } catch (error: unknown) {
      const err = error as Error;
      console.error("Connection error:", err);

      if (err.name === "NotFoundError") {
        setState({
          status: "error",
          message:
            "No device selected. Please try again and select your Faderpunk device.",
        });
      } else if (err.message?.includes("WebUSB")) {
        setState({ status: "error", message: err.message });
      } else {
        setState({
          status: "error",
          message: `Failed to connect: ${err.message || "Unknown error"}`,
        });
      }
    }
  }

  const showButton = state.status !== "update-available";
  const isLoading =
    state.status === "connecting" || state.status === "redirecting";

  return (
    <main className="flex min-h-screen min-w-screen items-center justify-center bg-gray-500">
      <div className="flex flex-col justify-center">
        <div className="border-pink-fp flex flex-col items-center justify-center gap-8 rounded-sm border-3 p-10 shadow-[0px_0px_11px_2px_#B7B2B240]">
          <img
            src="/img/fp-logo-alt.svg"
            alt="Faderpunk Logo"
            className="w-48"
          />

          {showButton && (
            <button
              onClick={connectAndRedirect}
              disabled={isLoading || !webUsbSupported}
              className="cursor-pointer rounded-sm bg-white px-8 py-2.5 text-sm font-semibold text-black shadow-[0px_0px_11px_2px_#B7B2B240] transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {state.status === "connecting" ? (
                <>
                  Connecting
                  <Spinner />
                </>
              ) : state.status === "redirecting" ? (
                <>
                  Redirecting
                  <Spinner />
                </>
              ) : (
                "Connect Device"
              )}
            </button>
          )}
        </div>

        <div className="mt-4 flex items-center justify-between gap-4">
          <a
            href={VERSION_PATH + "#/troubleshooting"}
            className="cursor-pointer text-center text-gray-400 underline hover:text-[#d4d4d8]"
          >
            Trouble connecting?
          </a>
          <a
            href={VERSION_PATH + "#/about"}
            className="cursor-pointer text-center text-gray-400 underline hover:text-[#d4d4d8]"
          >
            What is this?
          </a>
        </div>

        {state.status === "error" && (
          <div className="mt-4 rounded-sm border border-[#ff4444] bg-[#2a1a1a] p-4 text-[#ff6666]">
            {state.message}
          </div>
        )}

        {!webUsbSupported && state.status === "idle" && (
          <div className="mt-4 rounded-sm border border-[#ff4444] bg-[#2a1a1a] p-4 text-[#ff6666]">
            WebUSB is not supported in this browser. Please use Chrome, Edge, or
            another Chromium-based browser.
          </div>
        )}

        {state.status === "update-available" && (
          <div className="border-pink-fp mt-4 rounded-sm border bg-[#1a1a2a] p-5 text-center">
            <p className="mb-4 text-sm text-[#d4d4d8]">
              You're running firmware{" "}
              <span className="text-pink-fp font-semibold">
                v{state.currentVersion}
              </span>
              . Version{" "}
              <span className="text-pink-fp font-semibold">
                v{FIRMWARE_LATEST_VERSION}
              </span>{" "}
              is available.
            </p>
            <div className="flex justify-center gap-3">
              <button
                onClick={() => {
                  window.location.href = VERSION_PATH + "#/update";
                }}
                className="cursor-pointer rounded-sm bg-white px-6 py-2 text-xs font-semibold text-black transition-opacity hover:opacity-90"
              >
                Update Firmware
              </button>
              <button
                onClick={() => {
                  window.location.href = state.configuratorPath;
                }}
                className="cursor-pointer rounded-sm border border-gray-400 bg-transparent px-6 py-2 text-xs font-semibold text-gray-400 transition-opacity hover:text-[#d4d4d8] hover:opacity-90"
              >
                Continue Anyway
              </button>
            </div>
          </div>
        )}
      </div>
    </main>
  );
}

function Spinner() {
  return (
    <span className="ml-2 inline-block size-4 animate-spin rounded-full border-2 border-black/30 border-t-black align-middle" />
  );
}
