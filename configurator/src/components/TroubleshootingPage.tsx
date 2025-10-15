import { Link } from "react-router-dom";

import { Layout } from "./Layout";

export const TroubleshootingPage = () => (
  <Layout>
    <h2 className="text-yellow-fp mt-8 mb-4 text-lg font-bold uppercase">
      Use a compatible browser
    </h2>
    <p className="mb-8 text-lg font-semibold">
      Make sure you're using a compatible Browser (e.g.{" "}
      <a
        className="underline"
        href="https://www.google.com/intl/en_us/chrome/"
        target="_blank"
      >
        Chrome
      </a>
      ,{" "}
      <a
        className="underline"
        href="https://www.microsoft.com/en-us/edge/download"
        target="_blank"
      >
        Edge
      </a>
      ,{" "}
      <a
        className="underline"
        href="https://brave.com/download/"
        target="_blank"
      >
        Brave
      </a>
      ,{" "}
      <a
        className="underline"
        href="https://vivaldi.com/download/"
        target="_blank"
      >
        Vivaldi
      </a>
      ).
    </p>
    <h2 className="text-yellow-fp mt-8 mb-4 text-lg font-bold uppercase">
      Update your firmware
    </h2>
    <p className="mb-4 text-lg font-semibold">
      Please{" "}
      <Link className="underline" to="/update">
        update your firmware
      </Link>
    </p>
  </Layout>
);
