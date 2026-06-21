//! Headless FOMOD-choice replay + Collection rule→rank mapping (COLL-03).
//!
//! A Collection pins each scripted-installer mod's wizard answers as an `IChoices`
//! manifest (`{type:"fomod", options:[step{name,groups[group{name,choices[option]}]}]}`).
//! [`replay_choices`] converts that manifest into the SAME [`fomod::Selection`] the
//! interactive wizard would build, by NAME-matching every step → group → option against
//! the parsed [`FomodModule`] (RESEARCH Pattern 6). The caller then feeds the `Selection`
//! to the **same** [`fomod::resolve`] — there is no separate Collection install engine and
//! no per-mod wizard pops during a Collection install.
//!
//! HARD SAFETY RULE (RESEARCH A3 / Pitfall 4): if a manifest step/group/option name no
//! longer matches the mod's `ModuleConfig.xml` (the mod was updated since the Collection
//! pinned it), [`replay_choices`] returns a SPECIFIC [`NexusError`] — it NEVER silently
//! drops the choice and mis-installs. The caller surfaces this for a manual wizard pass.
//!
//! [`map_rules_to_ranks`] translates the Collection's `modRules[]` (`after`/`before`/
//! `conflicts`) plus per-mod `fileOverrides` onto the EXISTING Phase-2 conflict-rank model
//! (RESEARCH Pattern 7) — no new rules engine. `after` ⇒ the source gets a HIGHER rank
//! number (lower priority, loses file conflicts) than the reference; `before` ⇒ the
//! inverse. A rule whose `reference` matches no resolved mod is skipped, never fatal
//! (Pitfall 4 / T-04-09).

use std::collections::HashMap;

use fomod::{FomodModule, Selection};

use crate::collection::{Choices, CollectionModRule, ModReference, ModRuleType, SourceType};
use crate::error::NexusError;

/// Replay a Collection mod's pinned FOMOD [`Choices`] against the parsed [`FomodModule`],
/// producing the [`fomod::Selection`] the SAME [`fomod::resolve`] drives (COLL-03,
/// Pattern 6).
///
/// For each manifest step → group → option, the name is matched (case-sensitively, exactly
/// as authored) against the module's `installSteps`. A matched option is added to the
/// `Selection`'s `chosen` set and the flags it sets (`<conditionFlags>`) are accumulated
/// into `Selection.flags` — exactly what the live wizard does when the user picks it.
///
/// # Errors
/// Returns [`NexusError::Replay`] when a manifest step/group/option name does NOT exist in
/// the parsed module (a stale pin — the mod changed since the Collection captured it). The
/// caller surfaces this for a manual wizard pass; the choice is NEVER silently dropped.
pub fn replay_choices(module: &FomodModule, choices: &Choices) -> Result<Selection, NexusError> {
    let mut selection = Selection::default();

    let steps = module.steps.as_ref().map(|s| s.steps.as_slice()).unwrap_or(&[]);

    for step in &choices.options {
        let module_step = steps
            .iter()
            .find(|s| s.name == step.name)
            .ok_or_else(|| stale(format!("install step '{}' no longer exists", step.name)))?;

        let groups = module_step
            .groups
            .as_ref()
            .map(|g| g.groups.as_slice())
            .unwrap_or(&[]);

        for group in &step.groups {
            let module_group = groups
                .iter()
                .find(|g| g.name == group.name)
                .ok_or_else(|| {
                    stale(format!(
                        "group '{}' no longer exists in step '{}'",
                        group.name, step.name
                    ))
                })?;

            let plugins = module_group
                .plugins
                .as_ref()
                .map(|p| p.plugins.as_slice())
                .unwrap_or(&[]);

            for option in &group.choices {
                let module_plugin = plugins
                    .iter()
                    .find(|p| p.name == option.name)
                    .ok_or_else(|| {
                        stale(format!(
                            "option '{}' no longer exists in group '{}' (step '{}')",
                            option.name, group.name, step.name
                        ))
                    })?;

                // Mark the option chosen — keyed exactly as fomod::resolve looks it up.
                selection.chosen.insert((
                    step.name.clone(),
                    group.name.clone(),
                    option.name.clone(),
                ));

                // Accumulate the flags this option sets (what the live wizard does on pick).
                if let Some(cf) = &module_plugin.condition_flags {
                    for flag in &cf.flags {
                        selection.flags.insert(flag.name.clone(), flag.value.clone());
                    }
                }
            }
        }
    }

    Ok(selection)
}

/// Construct the specific stale-choice error (the mod changed since the Collection pinned
/// it). A dedicated message keeps the "fail clearly, never mis-install" contract visible.
fn stale(detail: String) -> NexusError {
    NexusError::Replay(format!(
        "collection FOMOD choice no longer matches the mod's installer: {detail}"
    ))
}

/// A relative-rank adjustment for one resolved mod, derived from the Collection's rules.
///
/// The orchestrator seeds every resolved mod with a baseline rank (e.g. manifest order)
/// and then applies these deltas: a higher `rank` number loses file conflicts (Phase-2
/// `conflict::resolve` semantics). `file_overrides` are the `dest_rel` paths this mod
/// force-wins regardless of rank.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RankAdjustment {
    /// How many positions to push this mod DOWN the rank ladder (higher number ⇒ later ⇒
    /// loses conflicts). Negative pushes it UP (earlier ⇒ wins). Summed across rules.
    pub rank_delta: i64,
    /// File paths this mod force-wins in conflict resolution (from `fileOverrides`).
    pub file_overrides: Vec<String>,
}

/// Map the Collection's `modRules[]` + per-mod `fileOverrides` onto the Phase-2 rank model
/// (RESEARCH Pattern 7) — NO new rules engine.
///
/// `key_for` resolves a [`ModReference`] (or a mod identity) to a stable key (e.g. the
/// resolved `modId`); callers key their mods the same way. The returned map carries one
/// [`RankAdjustment`] per mod that any rule or override touches:
///
/// * `after`  ⇒ the **source** gets a higher rank number than the reference (loses) —
///   `rank_delta += 1` on the source.
/// * `before` ⇒ the inverse — `rank_delta -= 1` on the source.
/// * `conflicts` ⇒ surfaced via the existing conflict view; the resulting rank decides the
///   winner. No delta is applied (the user resolves it), but both mods are recorded so the
///   conflict is visible.
/// * `requires` / `recommends` / `provides` ⇒ not ordering rules; ignored here.
/// * `file_overrides` (per mod) ⇒ recorded as force-win `dest_rel` paths.
///
/// A rule whose `source` or `reference` matches no resolved mod (per `key_for`) is SKIPPED
/// — never fatal (Pitfall 4 / T-04-09).
pub fn map_rules_to_ranks<F>(
    rules: &[CollectionModRule],
    file_overrides: &HashMap<String, Vec<String>>,
    mut key_for: F,
) -> HashMap<String, RankAdjustment>
where
    F: FnMut(&ModReference) -> Option<String>,
{
    let mut adjustments: HashMap<String, RankAdjustment> = HashMap::new();

    for rule in rules {
        // Skip a rule whose endpoints don't both resolve to a known mod (Pitfall 4).
        let (Some(source_key), Some(_reference_key)) =
            (key_for(&rule.source), key_for(&rule.reference))
        else {
            continue;
        };

        match rule.kind {
            ModRuleType::After => {
                adjustments.entry(source_key).or_default().rank_delta += 1;
            }
            ModRuleType::Before => {
                adjustments.entry(source_key).or_default().rank_delta -= 1;
            }
            ModRuleType::Conflicts => {
                // Record both endpoints so the conflict is visible; no ordering delta.
                adjustments.entry(source_key).or_default();
                if let Some(ref_key) = key_for(&rule.reference) {
                    adjustments.entry(ref_key).or_default();
                }
            }
            // Not ordering rules — left to the existing dependency/recommendation surface.
            ModRuleType::Requires | ModRuleType::Recommends | ModRuleType::Provides => {}
        }
    }

    // Per-mod fileOverrides force-win paths (Pattern 7 / fileOverrides).
    for (mod_key, paths) in file_overrides {
        if paths.is_empty() {
            continue;
        }
        adjustments
            .entry(mod_key.clone())
            .or_default()
            .file_overrides
            .extend(paths.iter().cloned());
    }

    adjustments
}

/// Whether a Collection mod's `source.type` is one NexTwist may auto-download (`nexus` or
/// `bundle`). Off-Nexus (`direct`/`browse`/`manual`) is surfaced as a manual step and NEVER
/// fetched (T-04-12). A small helper so the orchestrator's download loop reads clearly.
pub fn is_auto_fetchable(source: SourceType) -> bool {
    matches!(source, SourceType::Nexus | SourceType::Bundle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{ChoiceGroup, ChoiceOption, ChoiceStep};
    use std::fs;
    use std::path::Path;

    /// Write a `fomod/ModuleConfig.xml` under `root` and parse it into a `FomodModule`.
    fn module_from_xml(root: &Path, xml: &str) -> FomodModule {
        let fomod_dir = root.join("fomod");
        fs::create_dir_all(&fomod_dir).unwrap();
        fs::write(fomod_dir.join("ModuleConfig.xml"), xml).unwrap();
        fomod::parse_module_config(root).expect("fixture parses")
    }

    /// A two-option module: step "Main" / group "Pick" / options "A" (sets flag) and "B".
    const MODULE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<config>
  <moduleName>Test</moduleName>
  <installSteps order="Explicit">
    <installStep name="Main">
      <optionalFileGroups order="Explicit">
        <group name="Pick" type="SelectExactlyOne">
          <plugins order="Explicit">
            <plugin name="A">
              <description>option a</description>
              <files>
                <file source="a.esp" destination="a.esp" priority="0"/>
              </files>
              <conditionFlags>
                <flag name="picked">A</flag>
              </conditionFlags>
              <typeDescriptor><type name="Optional"/></typeDescriptor>
            </plugin>
            <plugin name="B">
              <description>option b</description>
              <files>
                <file source="b.esp" destination="b.esp" priority="0"/>
              </files>
              <typeDescriptor><type name="Optional"/></typeDescriptor>
            </plugin>
          </plugins>
        </group>
      </optionalFileGroups>
    </installStep>
  </installSteps>
</config>"#;

    fn choices_for(step: &str, group: &str, option: &str) -> Choices {
        Choices {
            kind: "fomod".to_string(),
            options: vec![ChoiceStep {
                name: step.to_string(),
                groups: vec![ChoiceGroup {
                    name: group.to_string(),
                    choices: vec![ChoiceOption {
                        name: option.to_string(),
                        idx: 0,
                    }],
                }],
            }],
        }
    }

    #[test]
    fn replay_drives_resolve_to_the_chosen_plan() {
        let tmp = tempfile::tempdir().unwrap();
        let module = module_from_xml(tmp.path(), MODULE_XML);

        // Replaying choice "A" must select A (not B) and set the "picked" flag.
        let sel = replay_choices(&module, &choices_for("Main", "Pick", "A"))
            .expect("a matching replay succeeds");
        assert!(sel.is_chosen("Main", "Pick", "A"));
        assert!(!sel.is_chosen("Main", "Pick", "B"));
        assert_eq!(sel.flags.get("picked").map(String::as_str), Some("A"));

        // Fed to the SAME resolver, the plan installs a.esp and NOT b.esp.
        let plan = fomod::resolve(&module, &sel).expect("resolve runs");
        let dests: Vec<String> = plan
            .iter()
            .map(|f| f.dest_rel.to_string_lossy().to_string())
            .collect();
        assert!(dests.iter().any(|d| d.contains("a.esp")), "a.esp installed: {dests:?}");
        assert!(!dests.iter().any(|d| d.contains("b.esp")), "b.esp NOT installed: {dests:?}");
    }

    #[test]
    fn stale_step_name_returns_specific_error_not_silent_install() {
        let tmp = tempfile::tempdir().unwrap();
        let module = module_from_xml(tmp.path(), MODULE_XML);

        // The mod no longer has a step named "Gone" — must error, never silently no-op.
        let err = replay_choices(&module, &choices_for("Gone", "Pick", "A"))
            .expect_err("a stale step name must error");
        assert!(matches!(err, NexusError::Replay(_)), "got {err:?}");
    }

    #[test]
    fn stale_option_name_returns_specific_error() {
        let tmp = tempfile::tempdir().unwrap();
        let module = module_from_xml(tmp.path(), MODULE_XML);

        // The option "C" never existed — a stale pin must surface, not silently mis-install.
        let err = replay_choices(&module, &choices_for("Main", "Pick", "C"))
            .expect_err("a stale option name must error");
        assert!(matches!(err, NexusError::Replay(_)), "got {err:?}");
    }

    fn reference_with_tag(tag: &str) -> ModReference {
        ModReference {
            tag: Some(tag.to_string()),
            ..Default::default()
        }
    }

    fn rule(kind: ModRuleType, source: &str, reference: &str) -> CollectionModRule {
        CollectionModRule {
            kind,
            source: reference_with_tag(source),
            reference: reference_with_tag(reference),
        }
    }

    #[test]
    fn after_gives_source_a_higher_rank_before_the_inverse() {
        // Resolve a reference to its `tag` string as the mod key.
        let key_for = |r: &ModReference| r.tag.clone();

        // "modA" loads AFTER "modB" ⇒ modA gets +1 (higher rank number = loses).
        let after = map_rules_to_ranks(
            &[rule(ModRuleType::After, "modA", "modB")],
            &HashMap::new(),
            key_for,
        );
        assert_eq!(after.get("modA").unwrap().rank_delta, 1);

        // "modA" loads BEFORE "modB" ⇒ modA gets -1 (lower rank number = wins).
        let before = map_rules_to_ranks(
            &[rule(ModRuleType::Before, "modA", "modB")],
            &HashMap::new(),
            key_for,
        );
        assert_eq!(before.get("modA").unwrap().rank_delta, -1);
    }

    #[test]
    fn rule_referencing_an_unresolved_mod_is_skipped_not_fatal() {
        // key_for resolves only "modA"; "ghost" resolves to None ⇒ the rule is skipped.
        let key_for = |r: &ModReference| match r.tag.as_deref() {
            Some("modA") => Some("modA".to_string()),
            _ => None,
        };
        let out = map_rules_to_ranks(
            &[rule(ModRuleType::After, "modA", "ghost")],
            &HashMap::new(),
            key_for,
        );
        // No adjustment recorded — the rule had an unresolved endpoint.
        assert!(out.is_empty(), "a rule with an unresolved endpoint is skipped: {out:?}");
    }

    #[test]
    fn file_overrides_are_recorded_as_force_win_paths() {
        let key_for = |r: &ModReference| r.tag.clone();
        let mut overrides = HashMap::new();
        overrides.insert("modA".to_string(), vec!["Data/x.dds".to_string()]);

        let out = map_rules_to_ranks(&[], &overrides, key_for);
        assert_eq!(
            out.get("modA").unwrap().file_overrides,
            vec!["Data/x.dds".to_string()]
        );
    }
}
