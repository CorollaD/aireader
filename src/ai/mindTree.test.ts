import { describe, expect, it } from "vitest";
import { allIds, layout, parseOutline, toMarkdown } from "./mindTree";

const outline = `Here is the map:
\`\`\`markdown
- Book
  - Part one [1]
    - Detail **bold** [2, 3]
  - Part two
\`\`\``;

describe("mind map outline", () => {
  it("parses nested items with citations", () => {
    const root = parseOutline(outline, "Title")!;
    expect(root.text).toBe("Book");
    expect(root.children.map((c) => c.text)).toEqual(["Part one", "Part two"]);
    const detail = root.children[0].children[0];
    expect(detail).toMatchObject({ text: "Detail bold", cites: [2, 3] });
    // Nodes that can be collapsed (those with children).
    expect(allIds(root)).toEqual(["0", "0.0"]);
  });

  it("gathers several top-level items under the title", () => {
    const root = parseOutline("- A\n- B", "Title")!;
    expect(root.text).toBe("Title");
    expect(root.children).toHaveLength(2);
    expect(parseOutline("no list here", "T")).toBeNull();
  });

  it("round-trips through Markdown", () => {
    const root = parseOutline(outline, "Title")!;
    expect(parseOutline(toMarkdown(root), "Title")).toEqual(root);
  });

  it("lays out children to the right without overlap", () => {
    const root = parseOutline(outline, "Title")!;
    const { nodes } = layout(root, new Set());
    const byText = (t: string) => nodes.find((n) => n.node.text === t)!;
    expect(byText("Part one").x).toBeGreaterThan(byText("Book").x);
    const [a, b] = [byText("Part one"), byText("Part two")];
    expect(Math.abs(a.y - b.y)).toBeGreaterThan(0);
    // Collapsing a node hides its children.
    const collapsed = layout(root, new Set([root.children[0].id]));
    expect(collapsed.nodes.some((n) => n.node.text === "Detail bold")).toBe(false);
  });
});
