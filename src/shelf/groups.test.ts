import { describe, expect, it } from "vitest";
import { childName, groupTree, inGroup, labelOf, parentOf } from "./groups";

describe("subgroups", () => {
  it("nests a name only under a group that exists", () => {
    expect(parentOf("技术-机器学习", ["技术", "技术-机器学习"])).toBe("技术");
    expect(parentOf("Sci-Fi", ["Sci-Fi", "History"])).toBeNull();
    expect(parentOf("技术", ["技术"])).toBeNull();
  });

  it("stops at two levels", () => {
    const groups = ["A", "A-B", "A-B-C"];
    expect(parentOf("A-B-C", groups)).toBe("A");
    expect(labelOf("A-B-C", "A")).toBe("B-C");
  });

  it("prefers the longest top-level group", () => {
    const groups = ["Sci-Fi", "Sci-Fi-Hard"];
    expect(parentOf("Sci-Fi-Hard", groups)).toBe("Sci-Fi");
    expect(labelOf("Sci-Fi-Hard", "Sci-Fi")).toBe("Hard");
  });

  it("builds a tree in shelf order", () => {
    expect(groupTree(["B", "A", "A-1", "B-2", "A-3"])).toEqual([
      { name: "B", children: ["B-2"] },
      { name: "A", children: ["A-1", "A-3"] },
    ]);
  });

  it("makes names from labels", () => {
    expect(childName("技术", " 机器学习 ")).toBe("技术-机器学习");
    expect(childName(null, " 技术 ")).toBe("技术");
  });

  it("counts a subgroup's books in its parent, not the reverse", () => {
    const groups = ["A", "A-B", "C"];
    expect(inGroup("A-B", "A", groups)).toBe(true);
    expect(inGroup("A", "A", groups)).toBe(true);
    expect(inGroup("A", "A-B", groups)).toBe(false);
    expect(inGroup("C", "A", groups)).toBe(false);
    expect(inGroup(null, "A", groups)).toBe(false);
  });
});
