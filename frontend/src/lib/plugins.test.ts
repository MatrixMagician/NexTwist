import { describe, expect, it } from "vitest";
import type { PluginInfo, PluginKind } from "$lib/api";
import { isMaster, kindBadge, reorder, touchesProtected, violatesMastersFirst } from "./plugins";

const p = (name: string, kind: PluginKind, over: Partial<PluginInfo> = {}): PluginInfo => ({
  name,
  kind,
  enabled: true,
  order: 0,
  medium: false,
  protected: false,
  ...over,
});

const numbered = (list: PluginInfo[]) => list.map((x, i) => ({ ...x, order: i }));

// masters first, then regular plugins — the invariant the engine enforces.
const list = numbered([
  p("Starfield.esm", "esm", { protected: true }),
  p("A.esm", "esm"),
  p("B.esm", "esm"),
  p("One.esp", "esp"),
  p("Two.esp", "esp"),
]);

describe("isMaster / kindBadge", () => {
  it("counts .esm and .esl as masters, .esp as regular", () => {
    expect(isMaster(p("x", "esm"))).toBe(true);
    expect(isMaster(p("x", "esl"))).toBe(true);
    expect(isMaster(p("x", "esp"))).toBe(false);
  });

  it("uppercases the kind badge", () => {
    expect(kindBadge("esp")).toBe("ESP");
  });
});

describe("violatesMastersFirst", () => {
  it("allows moves within the same group", () => {
    expect(violatesMastersFirst(list, 1, 1)).toBe(false); // A.esm <-> B.esm
    expect(violatesMastersFirst(list, 4, -1)).toBe(false); // Two.esp <-> One.esp
  });

  it("refuses moves that would cross the master/regular boundary", () => {
    expect(violatesMastersFirst(list, 2, 1)).toBe(true); // B.esm <-> One.esp
    expect(violatesMastersFirst(list, 3, -1)).toBe(true);
  });

  it("reports out-of-range moves as refused", () => {
    expect(violatesMastersFirst(list, 0, -1)).toBe(true);
    expect(violatesMastersFirst(list, list.length - 1, 1)).toBe(true);
  });
});

describe("reorder", () => {
  it("swaps and renumbers a legal move without mutating the input", () => {
    const next = reorder(list, 1, 1);
    expect(next).not.toBeNull();
    expect(next!.map((x) => x.name)).toEqual([
      "Starfield.esm",
      "B.esm",
      "A.esm",
      "One.esp",
      "Two.esp",
    ]);
    expect(next!.map((x) => x.order)).toEqual([0, 1, 2, 3, 4]);
    expect(list.map((x) => x.name)[1]).toBe("A.esm");
  });

  it("refuses a move that touches a protected master", () => {
    expect(reorder(list, 1, -1)).toBeNull();
    expect(touchesProtected(list, 1, -1)).toBe(true);
  });

  it("refuses a masters-first violation", () => {
    expect(reorder(list, 2, 1)).toBeNull();
  });

  it("refuses out-of-range moves", () => {
    expect(reorder(list, 0, -1)).toBeNull();
    expect(reorder(list, list.length - 1, 1)).toBeNull();
  });

  it("never produces a list with a regular plugin before a master", () => {
    for (let i = 0; i < list.length; i++) {
      for (const dir of [-1, 1] as const) {
        const next = reorder(list, i, dir);
        if (!next) continue;
        const firstRegular = next.findIndex((x) => !isMaster(x));
        const lastMaster = next.map(isMaster).lastIndexOf(true);
        expect(firstRegular === -1 || firstRegular > lastMaster).toBe(true);
      }
    }
  });
});
