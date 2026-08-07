import { describe, expect, it } from "vitest";
import type { FomodGroup, FomodOption, FomodProjection, GroupType } from "$lib/api";
import { fomodKey, flagsOf, preselected, selectionOf, stepValid, toggle } from "./fomod";

const opt = (name: string, over: Partial<FomodOption> = {}): FomodOption => ({
  name,
  description: "",
  image: null,
  default_type: "Optional",
  flags: [],
  ...over,
});

const group = (name: string, group_type: GroupType, options: FomodOption[]): FomodGroup => ({
  name,
  group_type,
  options,
});

const proj = (groups: FomodGroup[], stepName = "Step 1"): FomodProjection => ({
  module_name: "Test",
  steps: [{ name: stepName, conditional: false, groups }],
});

describe("fomodKey", () => {
  it("distinguishes identities that would collide under naive concatenation", () => {
    expect(fomodKey("a", "b", "c")).not.toBe(fomodKey("a b", "", "c"));
    expect(fomodKey("a", "b", "c")).toBe(fomodKey("a", "b", "c"));
  });
});

describe("preselected", () => {
  it("locks on Required options and every option of a SelectAll group", () => {
    const p = proj([
      group("Core", "SelectAny", [opt("Req", { default_type: "Required" }), opt("Maybe")]),
      group("All", "SelectAll", [opt("X"), opt("Y")]),
    ]);
    expect(preselected(p)).toEqual(
      new Set([
        fomodKey("Step 1", "Core", "Req"),
        fomodKey("Step 1", "All", "X"),
        fomodKey("Step 1", "All", "Y"),
      ]),
    );
  });
});

describe("toggle", () => {
  const g = group("G", "SelectExactlyOne", [opt("A"), opt("B")]);
  const step = proj([g]).steps[0];

  it("clears siblings in a radio group", () => {
    let chosen = toggle(new Set(), step, "Step 1", "G", "A", "SelectExactlyOne");
    chosen = toggle(chosen, step, "Step 1", "G", "B", "SelectExactlyOne");
    expect([...chosen]).toEqual([fomodKey("Step 1", "G", "B")]);
  });

  it("lets SelectAtMostOne toggle back to none", () => {
    const g2 = group("G", "SelectAtMostOne", [opt("A"), opt("B")]);
    const s2 = proj([g2]).steps[0];
    let chosen = toggle(new Set(), s2, "Step 1", "G", "A", "SelectAtMostOne");
    expect(chosen.size).toBe(1);
    chosen = toggle(chosen, s2, "Step 1", "G", "A", "SelectAtMostOne");
    expect(chosen.size).toBe(0);
  });

  it("re-selecting the same SelectExactlyOne option keeps it chosen", () => {
    let chosen = toggle(new Set(), step, "Step 1", "G", "A", "SelectExactlyOne");
    chosen = toggle(chosen, step, "Step 1", "G", "A", "SelectExactlyOne");
    expect([...chosen]).toEqual([fomodKey("Step 1", "G", "A")]);
  });

  it("checkbox groups toggle independently and never mutate the input set", () => {
    const g3 = group("G", "SelectAny", [opt("A"), opt("B")]);
    const s3 = proj([g3]).steps[0];
    const start = new Set<string>();
    const a = toggle(start, s3, "Step 1", "G", "A", "SelectAny");
    const ab = toggle(a, s3, "Step 1", "G", "B", "SelectAny");
    expect(start.size).toBe(0);
    expect(ab.size).toBe(2);
    expect(toggle(ab, s3, "Step 1", "G", "A", "SelectAny").size).toBe(1);
  });
});

describe("stepValid", () => {
  it("requires exactly one for SelectExactlyOne", () => {
    const p = proj([group("G", "SelectExactlyOne", [opt("A"), opt("B")])]);
    const step = p.steps[0];
    expect(stepValid(step, new Set())).toBe(false);
    expect(stepValid(step, new Set([fomodKey("Step 1", "G", "A")]))).toBe(true);
    expect(
      stepValid(step, new Set([fomodKey("Step 1", "G", "A"), fomodKey("Step 1", "G", "B")])),
    ).toBe(false);
  });

  it("requires at least one for SelectAtLeastOne and nothing for SelectAny", () => {
    const atLeast = proj([group("G", "SelectAtLeastOne", [opt("A")])]).steps[0];
    expect(stepValid(atLeast, new Set())).toBe(false);
    expect(stepValid(atLeast, new Set([fomodKey("Step 1", "G", "A")]))).toBe(true);

    const any = proj([group("G", "SelectAny", [opt("A")])]).steps[0];
    expect(stepValid(any, new Set())).toBe(true);
  });

  it("treats a missing step as valid (nothing to gate)", () => {
    expect(stepValid(null, new Set())).toBe(true);
  });
});

describe("flagsOf / selectionOf", () => {
  const p = proj([
    group("G", "SelectAny", [
      opt("A", { flags: [["fA", "on"]] }),
      opt("B", { flags: [["fB", "on"]] }),
    ]),
  ]);

  it("accumulates only the chosen options' flags", () => {
    expect(flagsOf(p, new Set([fomodKey("Step 1", "G", "A")]))).toEqual([["fA", "on"]]);
    expect(flagsOf(null, new Set())).toEqual([]);
  });

  it("emits identities in authored order alongside the flags", () => {
    const sel = selectionOf(
      p,
      new Set([fomodKey("Step 1", "G", "B"), fomodKey("Step 1", "G", "A")]),
    );
    expect(sel.chosen).toEqual([
      ["Step 1", "G", "A"],
      ["Step 1", "G", "B"],
    ]);
    expect(sel.flags).toEqual([
      ["fA", "on"],
      ["fB", "on"],
    ]);
  });

  it("is empty when nothing is parsed", () => {
    expect(selectionOf(null, new Set())).toEqual({ chosen: [], flags: [] });
  });
});
