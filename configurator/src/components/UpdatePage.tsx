import { Layout } from "./Layout";
import { UpdateGuide } from "./manual/UpdateGuide";

export const UpdatePage = () => {
  return (
    <Layout>
      <h2 className="text-yellow-fp mt-8 mb-4 text-lg font-bold uppercase">
        Firmware update
      </h2>
      <UpdateGuide />
    </Layout>
  );
};
