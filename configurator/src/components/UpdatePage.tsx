import { Layout } from "./Layout";
import { useStore } from "../store";

export const UpdatePage = () => {
  const { deviceVersion } = useStore();
  return (
    <Layout>
      <div className="text-lg/10">
        <p className="mb-12">
          Your device's version is v{deviceVersion}. To use this configurator
          please update your device. See the{" "}
          <a className="font-semibold underline" href="/manual#update-firmware">
            manual
          </a>{" "}
          on how to do it.
        </p>
      </div>
    </Layout>
  );
};
