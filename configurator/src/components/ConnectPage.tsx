import { Link } from "react-router-dom";

import { ButtonPrimary } from "./Button";
import { useStore } from "../store";

export const ConnectPage = () => {
  const { connect } = useStore();

  return (
    <main className="flex min-h-screen min-w-screen items-center justify-center bg-gray-500">
      <div className="flex flex-col justify-center">
        <div className="border-pink-fp flex flex-col items-center justify-center gap-8 rounded-sm border-3 p-10 shadow-[0px_0px_11px_2px_#B7B2B240]">
          <img src="/img/fp-logo-alt.svg" className="w-48" />
          <ButtonPrimary
            className="shadow-[0px_0px_11px_2px_#B7B2B240]"
            onPress={() => connect()}
          >
            Connect Device
          </ButtonPrimary>
        </div>
        <div className="flex items-center justify-between">
          <Link
            to="/troubleshooting"
            className="text-default-400 mt-4 cursor-pointer text-center underline"
          >
            Trouble connecting?
          </Link>
          <Link
            to="/about"
            className="text-default-400 mt-4 cursor-pointer text-center underline"
          >
            What is this?
          </Link>
        </div>
      </div>
    </main>
  );
};
