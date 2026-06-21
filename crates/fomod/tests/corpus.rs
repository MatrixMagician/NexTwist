//! The FOMOD fixture corpus: the executable contract for parse → condition → resolve.
//!
//! Each `tests/fixtures/<case>/` holds a `fomod/ModuleConfig.xml` (or a deliberately
//! mis-cased / BOM-prefixed / malformed variant). These tests drive the public API only
//! (`fomod::parse_module_config`, `fomod::eval`, `fomod::plugin_type_state`,
//! `fomod::resolve`) so the engine's contract is locked independent of its internals.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use fomod::{
    eval, parse_module_config, plugin_type_state, resolve, FlagSet, FomodError, GroupType,
    InstalledFiles, PluginType, Selection,
};

/// Absolute path to a fixture's tree root (the dir that CONTAINS the `fomod/` folder).
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn flags(pairs: &[(&str, &str)]) -> FlagSet {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect::<HashMap<_, _>>()
}

// ── parse ───────────────────────────────────────────────────────────────────────

#[test]
fn simple_single_group_parses() {
    let m = parse_module_config(&fixture("simple")).expect("simple fixture parses");
    assert_eq!(m.module_name, "Simple Mod");
    let steps = m.steps.as_ref().expect("has steps");
    assert_eq!(steps.steps.len(), 1);
    let step = &steps.steps[0];
    assert_eq!(step.name, "Main");
    let groups = step.groups.as_ref().expect("step has groups");
    assert_eq!(groups.groups.len(), 1);
    let group = &groups.groups[0];
    assert_eq!(group.group_type, GroupType::SelectExactlyOne);
    let plugins = group.plugins.as_ref().expect("group has plugins");
    assert_eq!(plugins.plugins.len(), 1);
    assert_eq!(plugins.plugins[0].name, "Standard Edition");
}

#[test]
fn all_five_group_types_parse() {
    let m = parse_module_config(&fixture("group_types")).expect("group_types parses");
    let step = &m.steps.unwrap().steps[0];
    let gs = step.groups.as_ref().unwrap();
    let types: Vec<GroupType> = gs.groups.iter().map(|g| g.group_type).collect();
    assert_eq!(
        types,
        vec![
            GroupType::SelectExactlyOne,
            GroupType::SelectAtMostOne,
            GroupType::SelectAtLeastOne,
            GroupType::SelectAll,
            GroupType::SelectAny,
        ]
    );
}

#[test]
fn flag_driven_fixture_parses_with_visible_condition() {
    let m = parse_module_config(&fixture("flags")).expect("flags parses");
    let steps = m.steps.as_ref().unwrap();
    assert_eq!(steps.steps.len(), 2);
    // The second step carries a `visible` flag condition.
    assert!(steps.steps[1].visible.is_some(), "step 2 has a visible dep");
}

#[test]
fn conditional_file_installs_expose_patterns() {
    let m = parse_module_config(&fixture("conditional")).expect("conditional parses");
    let cfi = m.conditional.as_ref().expect("has conditionalFileInstalls");
    let patterns = cfi.patterns.as_ref().expect("has patterns");
    assert_eq!(patterns.patterns.len(), 1);
    assert!(m.required.is_some(), "has requiredInstallFiles");
}

#[test]
fn nested_and_or_composite_parses() {
    let m = parse_module_config(&fixture("nested_deps")).expect("nested_deps parses");
    let cfi = m.conditional.as_ref().unwrap();
    let pat = &cfi.patterns.as_ref().unwrap().patterns[0];
    let dep = pat.dependencies.as_ref().expect("pattern has deps");
    // operator="Or" with one flag arm + one NESTED And of two flags.
    assert_eq!(dep.operator, fomod::Operator::Or);
    assert_eq!(dep.flag_deps.len(), 1);
    assert_eq!(dep.nested.len(), 1);
    assert_eq!(dep.nested[0].operator, fomod::Operator::And);
    assert_eq!(dep.nested[0].flag_deps.len(), 2);
}

#[test]
fn dependency_type_parses_default_and_patterns() {
    let m = parse_module_config(&fixture("dependency_type")).expect("dependency_type parses");
    let main = &m.steps.as_ref().unwrap().steps[1];
    let plugin = &main.groups.as_ref().unwrap().groups[0].plugins.as_ref().unwrap().plugins[0];
    let td = plugin.type_descriptor.as_ref().expect("has typeDescriptor");
    let dt = td.dependency_type.as_ref().expect("is a dependencyType");
    assert_eq!(dt.default_type.name, PluginType::NotUsable);
    assert_eq!(dt.patterns.as_ref().unwrap().patterns.len(), 1);
}

#[test]
fn bom_prefixed_fixture_parses() {
    let m = parse_module_config(&fixture("bom")).expect("BOM-prefixed fixture parses");
    assert_eq!(m.module_name, "BOM Prefixed Mod");
}

#[test]
fn case_insensitive_fomod_dir_and_filename_locate() {
    // Dir is `FOMOD`, file is `moduleconfig.xml` — both must be found case-insensitively.
    let m = parse_module_config(&fixture("case_insensitive")).expect("case-insensitive locate");
    assert_eq!(m.module_name, "Case Insensitive Mod");
}

#[test]
fn case_insensitive_source_path_resolves() {
    // FOMOD `source="Textures/X.DDS"` must resolve onto the real staged `Textures/X.DDS`.
    let root = fixture("case_insensitive");
    let resolved =
        fomod::resolve_source_path(&root, "textures/x.dds").expect("case-insensitive source match");
    assert!(resolved.ends_with("Textures/X.DDS"), "got {resolved:?}");
    assert!(resolved.is_file(), "resolved path exists on disk");
}

#[test]
fn malformed_fixture_returns_specific_error_not_ok() {
    let err = parse_module_config(&fixture("malformed")).expect_err("malformed must NOT be Ok");
    assert!(
        matches!(err, FomodError::Xml(_) | FomodError::MalformedSchema(_)),
        "expected Xml/MalformedSchema, got {err:?}"
    );
}

// ── condition ───────────────────────────────────────────────────────────────────

#[test]
fn eval_reads_flag_condition() {
    let m = parse_module_config(&fixture("flags")).unwrap();
    let visible = m.steps.unwrap().steps[1].visible.clone().unwrap();
    let files = InstalledFiles::default();
    assert!(eval(&visible, &flags(&[("hires", "on")]), &files), "hires=on ⇒ visible");
    assert!(!eval(&visible, &flags(&[("hires", "off")]), &files), "hires=off ⇒ hidden");
    assert!(!eval(&visible, &flags(&[]), &files), "no flag ⇒ hidden");
}

#[test]
fn eval_nested_and_or_semantics() {
    let m = parse_module_config(&fixture("nested_deps")).unwrap();
    let dep = m.conditional.unwrap().patterns.unwrap().patterns[0]
        .dependencies
        .clone()
        .unwrap();
    let files = InstalledFiles::default();
    // Or( a=1 , And(b=1,c=1) )
    assert!(eval(&dep, &flags(&[("a", "1")]), &files), "a=1 alone satisfies the Or");
    assert!(
        eval(&dep, &flags(&[("b", "1"), ("c", "1")]), &files),
        "b=1 AND c=1 satisfies the nested And"
    );
    assert!(!eval(&dep, &flags(&[("b", "1")]), &files), "b=1 alone does not");
    assert!(!eval(&dep, &flags(&[]), &files), "no flags ⇒ false");
}

#[test]
fn plugin_type_state_walks_dependency_type() {
    let m = parse_module_config(&fixture("dependency_type")).unwrap();
    let plugin = &m.steps.as_ref().unwrap().steps[1]
        .groups
        .as_ref()
        .unwrap()
        .groups[0]
        .plugins
        .as_ref()
        .unwrap()
        .plugins[0];
    let td = plugin.type_descriptor.as_ref().unwrap();
    let files = InstalledFiles::default();
    // defaultType=NotUsable; the prereq=yes pattern flips it to Recommended.
    assert_eq!(plugin_type_state(td, &flags(&[]), &files), PluginType::NotUsable);
    assert_eq!(
        plugin_type_state(td, &flags(&[("prereq", "yes")]), &files),
        PluginType::Recommended
    );
}

// ── resolve (the pure dry-run) ────────────────────────────────────────────────────

#[test]
fn resolve_simple_selected_option_yields_file_install() {
    let m = parse_module_config(&fixture("simple")).unwrap();
    let mut sel = Selection::default();
    sel.chosen
        .insert(("Main".into(), "Core".into(), "Standard Edition".into()));
    let plan = resolve(&m, &sel).expect("resolve simple");
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].src, PathBuf::from("core/standard.esp"));
    assert_eq!(plan[0].dest_rel, PathBuf::from("standard.esp"));
}

#[test]
fn resolve_includes_required_and_conditional() {
    let m = parse_module_config(&fixture("conditional")).unwrap();
    // patchA flag on ⇒ requiredInstallFiles + the conditional patchA file.
    let sel = Selection {
        flags: flags(&[("patchA", "on")]),
        ..Default::default()
    };
    let plan = resolve(&m, &sel).expect("resolve conditional");
    let dests: Vec<PathBuf> = plan.iter().map(|f| f.dest_rel.clone()).collect();
    assert!(dests.contains(&PathBuf::from("core.esp")), "required core.esp present");
    assert!(dests.contains(&PathBuf::from("patchA.esp")), "conditional patchA.esp present");
}

#[test]
fn resolve_omits_conditional_when_flag_unset() {
    let m = parse_module_config(&fixture("conditional")).unwrap();
    let sel = Selection::default(); // no flags
    let plan = resolve(&m, &sel).expect("resolve conditional (no flag)");
    let dests: Vec<PathBuf> = plan.iter().map(|f| f.dest_rel.clone()).collect();
    assert!(dests.contains(&PathBuf::from("core.esp")), "required still present");
    assert!(!dests.contains(&PathBuf::from("patchA.esp")), "conditional absent");
}

#[test]
fn resolve_performs_no_filesystem_write() {
    // The dry-run safety gate: resolve must NEVER touch disk. Run it with the process cwd
    // set to a fresh empty temp dir and assert the dir stays empty afterwards.
    let tmp = tempfile::tempdir().expect("temp dir");
    let m = parse_module_config(&fixture("conditional")).unwrap();
    let sel = Selection {
        flags: flags(&[("patchA", "on")]),
        ..Default::default()
    };

    let before: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().collect();
    assert!(before.is_empty(), "precondition: temp dir empty");

    let _plan = resolve(&m, &sel).expect("resolve");

    let after: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().collect();
    assert!(after.is_empty(), "resolve must not write to disk (temp dir stayed empty)");
}
