interface Props {
  name: string;
  className?: string;
  alt?: string;
}

export const Icon = ({ name, className = "w-6 h-6", ...props }: Props) => {
  const base = import.meta.env.BASE_URL || "/";
  const iconUrl = `${base}icons/${name}.svg`;

  return (
    <div
      className={`${className} bg-current`}
      style={{
        mask: `url(${iconUrl}) no-repeat center / contain`,
        WebkitMask: `url(${iconUrl}) no-repeat center / contain`,
      }}
      role="img"
      aria-label={props.alt || name}
    />
  );
};
