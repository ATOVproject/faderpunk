import { PropsWithChildren } from "react";

export const H2 = ({ children }: PropsWithChildren) => (
  <h2 className="text-yellow-fp mt-8 mb-4 text-lg font-bold uppercase">
    {children}
  </h2>
);

export const H3 = ({ children }: PropsWithChildren) => (
  <h2 className="mt-6 mb-2 font-bold">{children}</h2>
);
