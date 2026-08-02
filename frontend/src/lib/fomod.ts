/**
 * Pure FOMOD wizard selection logic (no Svelte state, no IO).
 *
 * The engine (`crates/fomod`) owns parse/condition/resolve; this module owns only the
 * wizard's *selection bookkeeping*: which option identities are chosen, what flags that
 * implies, and whether a step's group constraints are satisfied. Keeping it pure makes
 * the group-type rules (SelectExactlyOne / AtMostOne / AtLeastOne / All) unit-testable.
 */
import type { FomodProjection, FomodStep, GroupType, FomodSelection } from "$lib/api";

/** A stable key for a chosen option identity (the Set membership key). */
export const fomodKey = (step: string, group: string, option: string): string =>
  JSON.stringify([step, group, option]);

/** Options the author locks on: `Required` plugins and every option of a SelectAll group. */
export function preselected(proj: FomodProjection): Set<string> {
  const out = new Set<string>();
  for (const step of proj.steps) {
    for (const group of step.groups) {
      for (const opt of group.options) {
        if (opt.default_type === "Required" || group.group_type === "SelectAll") {
          out.add(fomodKey(step.name, group.name, opt.name));
        }
      }
    }
  }
  return out;
}

/** The accumulated flag set from every currently-chosen option's authored flags. */
export function flagsOf(
  proj: FomodProjection | null,
  chosen: ReadonlySet<string>,
): [string, string][] {
  const out: [string, string][] = [];
  if (!proj) return out;
  for (const step of proj.steps) {
    for (const group of step.groups) {
      for (const opt of group.options) {
        if (chosen.has(fomodKey(step.name, group.name, opt.name))) {
          for (const f of opt.flags) out.push(f);
        }
      }
    }
  }
  return out;
}

/** The selection payload the engine consumes (chosen identities + accumulated flags). */
export function selectionOf(
  proj: FomodProjection | null,
  chosen: ReadonlySet<string>,
): FomodSelection {
  const picks: [string, string, string][] = [];
  if (proj) {
    for (const step of proj.steps) {
      for (const group of step.groups) {
        for (const opt of group.options) {
          if (chosen.has(fomodKey(step.name, group.name, opt.name))) {
            picks.push([step.name, group.name, opt.name]);
          }
        }
      }
    }
  }
  return { chosen: picks, flags: flagsOf(proj, chosen) };
}

/** Whether a step's group-selection constraints (min/max) are satisfied. */
export function stepValid(step: FomodStep | null, chosen: ReadonlySet<string>): boolean {
  if (!step) return true;
  for (const group of step.groups) {
    const n = group.options.filter((o) =>
      chosen.has(fomodKey(step.name, group.name, o.name)),
    ).length;
    if (group.group_type === "SelectExactlyOne" && n !== 1) return false;
    if (group.group_type === "SelectAtLeastOne" && n < 1) return false;
  }
  return true;
}

/** True for the group types that behave like a radio (at most one selection). */
export const isRadioGroup = (t: GroupType | string): boolean =>
  t === "SelectExactlyOne" || t === "SelectAtMostOne";

/**
 * Toggle one option and return the resulting chosen-set (never mutates the input).
 * Radio groups clear their siblings first; `SelectAtMostOne` additionally allows
 * toggling the current pick back off to "none".
 */
export function toggle(
  chosen: ReadonlySet<string>,
  step: FomodStep | null,
  stepName: string,
  groupName: string,
  optionName: string,
  groupType: GroupType | string,
): Set<string> {
  const key = fomodKey(stepName, groupName, optionName);
  const next = new Set(chosen);
  if (isRadioGroup(groupType)) {
    const g = step?.groups.find((x) => x.name === groupName);
    if (g) for (const o of g.options) next.delete(fomodKey(stepName, groupName, o.name));
    // Toggling the same AtMostOne option off leaves the group empty ("none").
    if (!(groupType === "SelectAtMostOne" && chosen.has(key))) next.add(key);
  } else if (next.has(key)) {
    next.delete(key);
  } else {
    next.add(key);
  }
  return next;
}
