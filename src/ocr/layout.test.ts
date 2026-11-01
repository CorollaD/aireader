import { describe, expect, it } from "vitest";
import { readingOrder, toRuns } from "./layout";
import type { TextRun } from "../types";

const run = (text: string, x: number, y: number, w: number, h = 0.02): TextRun => ({ text, x, y, w, h });
const order = (runs: TextRun[]) => readingOrder(runs).map((r) => r.text);

describe("reading order", () => {
  it("keeps a single column from top to bottom", () => {
    const page = [run("二", 0.1, 0.2, 0.8), run("一", 0.1, 0.1, 0.8), run("三", 0.1, 0.3, 0.3)];
    expect(order(page)).toEqual(["一", "二", "三"]);
  });

  it("reads a two-column page column by column", () => {
    const page = [];
    for (let i = 0; i < 4; i++) page.push(run(`左${i}`, 0.05, 0.1 + i * 0.05, 0.4), run(`右${i}`, 0.55, 0.1 + i * 0.05, 0.4));
    expect(order(page)).toEqual(["左0", "左1", "左2", "左3", "右0", "右1", "右2", "右3"]);
  });

  it("keeps titles and full-width lines around the columns", () => {
    const page = [run("标题", 0.1, 0.02, 0.8), run("页脚", 0.1, 0.9, 0.8)];
    for (let i = 0; i < 3; i++) page.push(run(`左${i}`, 0.05, 0.1 + i * 0.05, 0.4), run(`右${i}`, 0.55, 0.1 + i * 0.05, 0.4));
    expect(order(page)).toEqual(["标题", "左0", "左1", "左2", "右0", "右1", "右2", "页脚"]);
  });

  it("reads a line left to right when its boxes start at different heights", () => {
    const page = [run("硬件和软件要求", 0.2, 0.121, 0.25, 0.03), run("1.5", 0.13, 0.127, 0.05, 0.023)];
    expect(order(page)).toEqual(["1.5", "硬件和软件要求"]);
  });

  it("is not fooled by a short note beside a single column", () => {
    const page = [run("正文一", 0.05, 0.1, 0.4), run("页码", 0.8, 0.12, 0.1), run("正文二", 0.05, 0.2, 0.4)];
    expect(order(page)).toEqual(["正文一", "页码", "正文二"]);
  });
});

describe("runs from recognised lines", () => {
  const box = (x: number, y: number, w: number, h: number) => [
    [x, y],
    [x + w, y],
    [x + w, y + h],
    [x, y + h],
  ];

  it("converts pixels to fractions and drops empty lines", () => {
    const runs = toRuns(
      [
        { text: "正文", poly: box(100, 200, 500, 40) },
        { text: "  ", poly: box(0, 0, 10, 10) },
      ],
      1000,
      2000,
    );
    expect(runs).toEqual([{ text: "正文", x: 0.1, y: 0.1, w: 0.5, h: 0.02 }]);
  });

  it("takes the bounds of a slanted line", () => {
    const [r] = toRuns(
      [
        {
          text: "斜",
          poly: [
            [10, 20],
            [110, 30],
            [108, 50],
            [8, 40],
          ],
        },
      ],
      1000,
      1000,
    );
    expect(r).toMatchObject({ x: 0.008, y: 0.02, w: 0.102, h: 0.03 });
  });

  it("handles a page with nothing on it", () => {
    expect(toRuns([], 1000, 1000)).toEqual([]);
  });
});
