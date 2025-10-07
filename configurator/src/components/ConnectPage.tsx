import { Link, useNavigate } from "react-router-dom";

import { ButtonPrimary } from "./Button";
import { useStore } from "../store";

export const ConnectPage = () => {
  const { connect } = useStore();
  const navigate = useNavigate();

  return (
    <main className="flex min-h-screen min-w-screen items-center justify-center bg-gray-500">
      <div className="flex flex-col justify-center">
        <div className="border-pink-fp flex flex-col items-center justify-center gap-8 rounded-sm border-3 p-10 shadow-[0px_0px_11px_2px_#B7B2B240]">
          <img src="/img/fp-logo-alt.svg" className="w-48" />
          <ButtonPrimary
            className="shadow-[0px_0px_11px_2px_#B7B2B240]"
            onPress={() => connect(navigate)}
          >
            Connect Device
          </ButtonPrimary>
        </div>
        <Link
          to="/about"
          className="text-default-400 mt-4 cursor-pointer underline"
        >
          What is this?
        </Link>
      </div>
    </main>
  );
};
