import { PropsWithChildren } from "react";
import { type LinkProps, Link as RRLink } from "react-router-dom";

interface HProps {
  id?: string;
}

export const H2 = ({ children, id }: PropsWithChildren<HProps>) => (
  <h2 className="text-yellow-fp mt-8 mb-4 text-xl font-bold uppercase" id={id}>
    {children}
  </h2>
);

export const H3 = ({ children, id }: PropsWithChildren<HProps>) => (
  <h3 className="mt-6 mb-2 text-lg font-bold" id={id}>
    {children}
  </h3>
);

export const H4 = ({ children, id }: PropsWithChildren<HProps>) => (
  <h4 className="mt-6 mb-2 font-bold" id={id}>
    {children}
  </h4>
);

export const H5 = ({ children }: PropsWithChildren<HProps>) => (
  <h4 className="mt-4 font-semibold italic">{children}</h4>
);

export const List = ({ children }: PropsWithChildren<HProps>) => (
  <ul className="my-3 ml-3 list-inside list-disc">{children}</ul>
);

export const Link = (props: LinkProps) => (
  <RRLink className="font-semibold underline" {...props} />
);
