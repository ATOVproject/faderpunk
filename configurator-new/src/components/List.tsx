import React, { forwardRef } from "react";
import classNames from "classnames";

import styles from "./List.module.css";

export interface Props {
  children: React.ReactNode;
  columns?: number;
  style?: React.CSSProperties;
  horizontal?: boolean;
}

export const List = forwardRef<HTMLUListElement, Props>(
  ({ children, style }: Props, ref) => {
    return (
      <ul
        ref={ref}
        style={
          {
            ...style,
          } as React.CSSProperties
        }
        className={classNames("flex", styles.List, styles.horizontal)}
      >
        {children}
      </ul>
    );
  },
);
