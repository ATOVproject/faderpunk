import type { PropsWithChildren } from "react";
import { useLocation } from "react-router-dom";

import { Footer } from "./Footer";

export const Layout = ({ children }: PropsWithChildren) => {
  const location = useLocation();
  return (
    <main className="flex min-h-screen flex-col bg-gray-500 text-white">
      <div className="mx-auto flex w-full max-w-6xl flex-grow flex-col py-14">
        <div className="flex-grow">
          <div className="mb-8 text-center">
            <img src="/img/fp-logo.svg" className="inline w-64" />
            <h1 className="font-vox mt-3 text-xl font-semibold tracking-wider uppercase">
              {location.pathname.substring(1)}
            </h1>
          </div>
          {children}
          <Footer />
        </div>
      </div>
    </main>
  );
};
