import { Layout } from "./Layout";
import { useStore } from "../store";
import { UpdateGuide } from "./manual/UpdateGuide";
import { ButtonPrimary } from "./Button";

export const UpdatePage = () => {
  const { deviceVersion } = useStore();
  return (
    <Layout>
      <h2 className="text-yellow-fp mt-8 mb-4 text-lg font-bold uppercase">
        Please update your firmware
      </h2>
      <p className="mb-4 text-lg font-semibold">
        Your device's version is{" "}
        {deviceVersion ? `v${deviceVersion}` : "unknown"}. To use this
        configurator, please update your device.
      </p>
      <p className="mb-8 text-lg font-semibold">
        Yes, we know it's annoying, but please bear with us, it's a super quick
        process and will be worth your while :)
      </p>
      <ButtonPrimary
        as="a"
        href="https://github.com/ATOVproject/faderpunk/releases/download/faderpunk-v1.3.0/faderpunk.uf2"
      >
        Download v1.3.0
      </ButtonPrimary>
      <UpdateGuide />
    </Layout>
  );
};
