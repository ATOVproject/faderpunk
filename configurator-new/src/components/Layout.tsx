import type { PropsWithChildren } from "react";

export const Layout = ({ children }: PropsWithChildren) => (
  <main className="min-h-screen bg-gray-500 text-white">
    <div className="mx-auto max-w-6xl px-8 py-14">{children}</div>
  </main>
);
