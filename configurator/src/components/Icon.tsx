interface Props {
  name: string;
  className?: string;
  alt?: string;
}

export const Icon = ({ name, className = "w-6 h-6", ...props }: Props) => (
  <div
    className={`${className} bg-current`}
    style={{
      mask: `url(/icons/${name}.svg) no-repeat center / contain`,
      WebkitMask: `url(/icons/${name}.svg) no-repeat center / contain`,
    }}
    role="img"
    aria-label={props.alt || name}
  />
);
