import { type ReactNode } from "react";
import ReactMarkdown from "react-markdown";

interface MdProps {
  children: ReactNode;
}

export const Md = ({ children }: MdProps) => {
  if (typeof children !== "string") return <>{children}</>;

  return (
    <ReactMarkdown
      components={{
        p: ({ children }) => <>{children}</>,
        strong: ({ children }) => (
          <strong className="font-semibold">{children}</strong>
        ),
        em: ({ children }) => <em>{children}</em>,
        code: ({ children }) => (
          <code className="rounded bg-white/10 px-1 py-0.5 text-sm">
            {children}
          </code>
        ),
        ul: ({ children }) => (
          <ul className="my-1 ml-3 list-inside list-disc">{children}</ul>
        ),
        ol: ({ children }) => (
          <ol className="my-1 ml-3 list-inside list-decimal">{children}</ol>
        ),
        a: ({ href, children }) => (
          <a
            className="font-semibold underline"
            href={href}
            target="_blank"
            rel="noopener noreferrer"
          >
            {children}
          </a>
        ),
      }}
    >
      {children}
    </ReactMarkdown>
  );
};
