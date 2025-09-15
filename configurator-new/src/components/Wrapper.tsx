import React from "react";
import classNames from "classnames";

import styles from "./Wrapper.module.css";

interface Props {
  children: React.ReactNode;
  style?: React.CSSProperties;
}

export function Wrapper({ children, style }: Props) {
  return (
    <div className={classNames(styles.Wrapper, styles.center)} style={style}>
      {children}
    </div>
  );
}
