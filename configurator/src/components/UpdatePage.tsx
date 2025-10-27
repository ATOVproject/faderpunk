import semverLt from "semver/functions/lt";

import { FIRMWARE_MIN_SUPPORTED } from "../consts";
import { useStore } from "../store";
import { Layout } from "./Layout";
import { UpdateGuide } from "./manual/UpdateGuide";

export const UpdatePage = () => {
  const { deviceVersion } = useStore();

  const updateNecessary =
    deviceVersion && semverLt(deviceVersion, FIRMWARE_MIN_SUPPORTED);

  return (
    <Layout>
      <h2 className="text-yellow-fp mt-8 mb-4 text-lg font-bold uppercase">
        {updateNecessary ? "Please update your firmware" : "Firmware update"}
      </h2>
      {updateNecessary ? (
        <>
          <p className="mb-4 text-lg font-semibold">
            Your device's version is{" "}
            {deviceVersion ? `v${deviceVersion}` : "unknown"}. To use this
            configurator, please update your device.
          </p>
          <p className="mb-8 text-lg font-semibold">
            Yes, we know it's annoying, but please bear with us, it's a super
            quick process and will be worth your while :)
          </p>
        </>
      ) : null}
      <UpdateGuide />
    </Layout>
  );
};
