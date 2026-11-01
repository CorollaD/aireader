import { beforeEach, describe, expect, it } from "vitest";
import { setLanguage } from "../i18n";
import { formatInterval } from "./intervals";

describe("card intervals", () => {
  beforeEach(() => setLanguage("zh-CN"));

  it("uses the largest unit that fits", () => {
    expect(formatInterval(5)).toBe("1 分钟");
    expect(formatInterval(600)).toBe("10 分钟");
    expect(formatInterval(7200)).toBe("2 小时");
    expect(formatInterval(3 * 86_400)).toBe("3 天");
    expect(formatInterval(60 * 86_400)).toBe("2 个月");
    expect(formatInterval(550 * 86_400)).toBe("1.5 年");
  });
});
