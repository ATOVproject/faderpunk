interface Props {
  name: string;
  className?: string;
  alt?: string;
}

export const Icon = ({ name, className = "w-6 h-6", alt }: Props) => (
  <img src={`/icons/${name}.svg`} alt={alt || name} className={className} />
);
