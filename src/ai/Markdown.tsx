// A small Markdown renderer for AI answers: headings, lists, quotes, code, bold/italic/inline
// code, and [n] citations. It builds React elements (never raw HTML), so model output cannot
// inject markup.

import type { ReactNode } from "react";

/** Citation numbers that exist, and what clicking one does. */
export interface Cite {
  count: number;
  onCite: (n: number) => void;
  title?: (n: number) => string;
}

function inline(text: string, cite?: Cite): ReactNode[] {
  const out: ReactNode[] = [];
  const re = /(\*\*[^*]+\*\*|`[^`]+`|\*[^*\s][^*]*\*|\[\d+(?:\s*[,，]\s*\d+)*\])/g;
  let last = 0;
  let m: RegExpExecArray | null;
  let i = 0;
  while ((m = re.exec(text))) {
    if (m.index > last) out.push(text.slice(last, m.index));
    const t = m[0];
    if (t.startsWith("**")) out.push(<strong key={i++}>{t.slice(2, -2)}</strong>);
    else if (t.startsWith("`")) out.push(<code key={i++}>{t.slice(1, -1)}</code>);
    else if (t.startsWith("[")) {
      const ns = t
        .slice(1, -1)
        .split(/[,，]/)
        .map((x) => Number(x.trim()));
      if (cite && ns.every((n) => n >= 1 && n <= cite.count)) {
        for (const n of ns) {
          out.push(
            <button key={i++} className="cite" title={cite.title?.(n)} onClick={() => cite.onCite(n)}>
              {n}
            </button>,
          );
        }
      } else out.push(t);
    } else out.push(<em key={i++}>{t.slice(1, -1)}</em>);
    last = m.index + t.length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

export default function Markdown({ text, cite }: { text: string; cite?: Cite }) {
  const span = (t: string) => inline(t, cite);
  const lines = text.replace(/\r/g, "").split("\n");
  const blocks: ReactNode[] = [];
  let i = 0;
  let key = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) {
      i++;
      continue;
    }
    if (line.startsWith("```")) {
      const code: string[] = [];
      i++;
      while (i < lines.length && !lines[i].startsWith("```")) code.push(lines[i++]);
      i++;
      blocks.push(<pre key={key++}>{code.join("\n")}</pre>);
      continue;
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      const level = Math.min(heading[1].length + 2, 6);
      const Tag = `h${level}` as "h3";
      blocks.push(<Tag key={key++}>{span(heading[2])}</Tag>);
      i++;
      continue;
    }
    if (/^\s*([-*+]|\d+[.)])\s+/.test(line)) {
      const ordered = /^\s*\d/.test(line);
      const items: ReactNode[] = [];
      while (i < lines.length && /^\s*([-*+]|\d+[.)])\s+/.test(lines[i])) {
        items.push(<li key={items.length}>{span(lines[i].replace(/^\s*([-*+]|\d+[.)])\s+/, ""))}</li>);
        i++;
      }
      blocks.push(ordered ? <ol key={key++}>{items}</ol> : <ul key={key++}>{items}</ul>);
      continue;
    }
    if (line.startsWith(">")) {
      const quote: string[] = [];
      while (i < lines.length && lines[i].startsWith(">")) quote.push(lines[i++].replace(/^>\s?/, ""));
      blocks.push(<blockquote key={key++}>{span(quote.join(" "))}</blockquote>);
      continue;
    }
    const para: string[] = [];
    while (i < lines.length && lines[i].trim() && !/^(#{1,6}\s|```|>|\s*([-*+]|\d+[.)])\s)/.test(lines[i])) para.push(lines[i++]);
    blocks.push(<p key={key++}>{span(para.join("\n"))}</p>);
  }
  return <div className="md">{blocks}</div>;
}
