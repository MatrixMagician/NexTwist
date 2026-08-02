import { describe, expect, it } from "vitest";
import { fmtBytes, pct } from "./format";

describe("pct", () => {
  it("is null when the total is unknown or zero", () => {
    expect(pct({ downloaded: 10, total: null })).toBe(null);
    expect(pct({ downloaded: 10, total: 0 })).toBe(null);
  });

  it("floors the percentage", () => {
    expect(pct({ downloaded: 1, total: 3 })).toBe(33);
    expect(pct({ downloaded: 0, total: 100 })).toBe(0);
    expect(pct({ downloaded: 100, total: 100 })).toBe(100);
  });
});

describe("fmtBytes", () => {
  it("steps units at the 1024 boundary", () => {
    expect(fmtBytes(0)).toBe("0 B");
    expect(fmtBytes(1023)).toBe("1023 B");
    expect(fmtBytes(1024)).toBe("1.0 KB");
    expect(fmtBytes(1024 * 1024 - 1)).toBe("1024.0 KB");
    expect(fmtBytes(1024 * 1024)).toBe("1.0 MB");
    expect(fmtBytes(1024 ** 3)).toBe("1.00 GB");
    expect(fmtBytes(3 * 1024 ** 3 + 512 * 1024 ** 2)).toBe("3.50 GB");
  });
});
