import { type ReactNode } from "react";
import ReactMarkdown from "react-markdown";

interface MdProps {
  children: ReactNode;
}

type Segment =
  | { kind: "md"; text: string }
  | { kind: "table"; headers: string[]; rows: string[][] };

/** Split GFM pipe-tables out so we can render them without remark-gfm. */
function splitMarkdownTables(src: string): Segment[] {
  const lines = src.split("\n");
  const out: Segment[] = [];
  let buf: string[] = [];

  const flushMd = () => {
    if (!buf.length) return;
    out.push({ kind: "md", text: buf.join("\n") });
    buf = [];
  };

  const isRow = (line: string) => /^\s*\|.*\|\s*$/.test(line);
  const isSep = (line: string) =>
    /^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)+\|?\s*$/.test(line);

  const cells = (line: string) =>
    line
      .trim()
      .replace(/^\|/, "")
      .replace(/\|$/, "")
      .split("|")
      .map((c) => c.trim());

  let i = 0;
  while (i < lines.length) {
    if (
      isRow(lines[i]) &&
      i + 1 < lines.length &&
      isSep(lines[i + 1])
    ) {
      flushMd();
      const headers = cells(lines[i]);
      i += 2;
      const rows: string[][] = [];
      while (i < lines.length && isRow(lines[i]) && !isSep(lines[i])) {
        rows.push(cells(lines[i]));
        i++;
      }
      out.push({ kind: "table", headers, rows });
      continue;
    }
    buf.push(lines[i]);
    i++;
  }
  flushMd();
  return out;
}

const mdComponents = {
  p: ({ children }: { children?: ReactNode }) => (
    <p className="mb-2 last:mb-0">{children}</p>
  ),
  h4: ({ children }: { children?: ReactNode }) => (
    <h4 className="mt-6 mb-2 font-bold">{children}</h4>
  ),
  strong: ({ children }: { children?: ReactNode }) => (
    <strong className="font-semibold">{children}</strong>
  ),
  em: ({ children }: { children?: ReactNode }) => <em>{children}</em>,
  code: ({ children }: { children?: ReactNode }) => (
    <code className="rounded bg-white/10 px-1 py-0.5 text-sm">{children}</code>
  ),
  ul: ({ children }: { children?: ReactNode }) => (
    <ul className="my-1 ml-3 list-inside list-disc">{children}</ul>
  ),
  ol: ({ children }: { children?: ReactNode }) => (
    <ol className="my-1 ml-3 list-inside list-decimal">{children}</ol>
  ),
  a: ({ href, children }: { href?: string; children?: ReactNode }) => (
    <a
      className="font-semibold underline"
      href={href}
      target="_blank"
      rel="noopener noreferrer"
    >
      {children}
    </a>
  ),
};

const InlineMd = ({ text }: { text: string }) => (
  <ReactMarkdown
    components={{
      ...mdComponents,
      // Table cells: keep inline — no wrapping <p>
      p: ({ children }) => <>{children}</>,
    }}
  >
    {text}
  </ReactMarkdown>
);

const ManualTable = ({
  headers,
  rows,
}: {
  headers: string[];
  rows: string[][];
}) => (
  <div className="my-3 overflow-x-auto">
    <table className="w-full min-w-[20rem] border-collapse text-left text-sm">
      <thead>
        <tr className="border-b border-white/20">
          {headers.map((h) => (
            <th
              key={h}
              className="px-2 py-1.5 font-vox font-semibold whitespace-nowrap"
            >
              <InlineMd text={h} />
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {rows.map((row, ri) => (
          <tr key={ri} className="border-b border-white/10 align-top">
            {headers.map((_, ci) => (
              <td key={ci} className="px-2 py-1.5">
                <InlineMd text={row[ci] ?? ""} />
              </td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  </div>
);

export const Md = ({ children }: MdProps) => {
  if (typeof children !== "string") return <>{children}</>;

  const segments = splitMarkdownTables(children);
  return (
    <div className="manual-md">
      {segments.map((seg, i) =>
        seg.kind === "table" ? (
          <ManualTable key={i} headers={seg.headers} rows={seg.rows} />
        ) : (
          <ReactMarkdown key={i} components={mdComponents}>
            {seg.text}
          </ReactMarkdown>
        ),
      )}
    </div>
  );
};
