import { type LinkProps, Link as RRLink } from "react-router-dom";

export { H2, H3, H4, H5, List } from "./Headings";

export const Link = (props: LinkProps) => (
  <RRLink className="font-semibold underline" {...props} />
);
