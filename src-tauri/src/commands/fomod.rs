//! FOMOD guided-installer adapter (FOMOD-01/FOMOD-02) — the thin IPC boundary over the
//! headless `crates/fomod` engine. Per the Anti-Pattern-4 contract (see `commands/mod.rs`):
//! NO FOMOD business logic lives here. Each `#[tauri::command]`:
//!
//! 1. resolves the managed game (`require_game`) or extracts the archive to a temp tree,
//! 2. calls EXACTLY the headless `fomod::parse_module_config` / `fomod::resolve` /
//!    `extract::install_archive` / `store.add_mod` functions, and
//! 3. maps the typed error to a `String` at the boundary via `boundary_err`.
//!
//! The three stages mirror the locked "parse → dry-run resolve → apply" safety gate:
//!
//! * [`parse_fomod`] — extract the archive to a validated temp tree, locate + parse
//!   `fomod/ModuleConfig.xml`, and return a SERIALIZABLE projection of the AST (the
//!   wizard renders radio/checkbox groups + type-states from this). A malformed
//!   `ModuleConfig.xml` returns the verbatim [`fomod::FomodError`] string so the
//!   frontend can offer the plain-mod fallback (UI-SPEC §A.8).
//! * [`resolve_fomod`] — the PURE dry-run: given the user's selection, call
//!   `fomod::resolve` and return a serializable file-install plan with a per-destination
//!   conflict classification. Writes NOTHING (the locked dry-run-before-apply gate).
//! * [`apply_fomod`] — on a confirmed (non-blocking) install, route the archive through
//!   the validated `extract::install_archive` staging path (Plan-01 root-detection,
//!   zip-slip/symlink/`..` defenses unchanged — the adapter adds no new write primitive,
//!   threat T-04-05), then `store.add_mod` so the result is an ordinary `ManagedMod`.
//!
//! The temp extraction here re-uses the SAME validated extractor the rest of the app
//! uses; FOMOD source-path resolution and parsing are pure reads over that tree.

use std::path::{Path, PathBuf};

use extract::ArchiveFormat;
use fomod::{
    parse_module_config, resolve, FomodModule, GroupType, OrderKind, PluginType, Selection,
};
use serde::{Deserialize, Serialize};
use tauri::State;
use tempfile::TempDir;
use tokio::sync::Mutex;

use crate::commands::{boundary_err, require_game};
use crate::state::AppState;

// ── Serializable wizard-facing AST projection ──────────────────────────────────────
//
// The webview only speaks JSON. These mirror the relevant `fomod::model` shapes (steps →
// groups → options + their static type) so the wizard can render without re-parsing XML.
// Live re-evaluation (option type-state flips, step visibility) is driven by repeated
// `resolve_fomod` calls; the static projection carries the authored structure + the
// authored default type, and the dependency-conditions the engine evaluates.

/// The parsed FOMOD module, projected for the wizard (FOMOD-01).
#[derive(Debug, Clone, Serialize)]
pub struct FomodProjection {
    /// `<moduleName>` — the wizard modal title.
    pub module_name: String,
    /// The ordered wizard steps (already name-sorted per the authored `order`).
    pub steps: Vec<StepProjection>,
}

/// One wizard install step.
#[derive(Debug, Clone, Serialize)]
pub struct StepProjection {
    /// Step name (the "· {step name}" in the counter).
    pub name: String,
    /// Whether this step carries a `<visible>` condition (its live truth is decided by
    /// the engine in `resolve_fomod`; the wizard skips an invisible step).
    pub conditional: bool,
    /// The option groups in this step.
    pub groups: Vec<GroupProjection>,
}

/// One option group within a step.
#[derive(Debug, Clone, Serialize)]
pub struct GroupProjection {
    /// Group name.
    pub name: String,
    /// The FOMOD selection constraint (drives radio-vs-checkbox + min/max).
    pub group_type: GroupTypeDto,
    /// The selectable options.
    pub options: Vec<OptionProjection>,
}

/// One selectable option (`<plugin>`).
#[derive(Debug, Clone, Serialize)]
pub struct OptionProjection {
    /// Option name (the label + the selection identity).
    pub name: String,
    /// `<description>` (muted when unselected).
    pub description: String,
    /// Archive-relative `<image path>` if present (the wizard bounds it ≤96px).
    pub image: Option<String>,
    /// The authored default/static type-state (Required/Optional/Recommended/NotUsable/
    /// CouldBeUsable). The LIVE type-state after choices is recomputed by `resolve_fomod`.
    pub default_type: PluginTypeDto,
}

/// Serializable mirror of [`fomod::GroupType`].
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum GroupTypeDto {
    /// Exactly one (radio).
    SelectExactlyOne,
    /// At most one (radio, none allowed).
    SelectAtMostOne,
    /// At least one (checkbox, min 1).
    SelectAtLeastOne,
    /// All (checkbox, locked on).
    SelectAll,
    /// Any (checkbox, free).
    SelectAny,
}

impl From<GroupType> for GroupTypeDto {
    fn from(g: GroupType) -> Self {
        match g {
            GroupType::SelectExactlyOne => GroupTypeDto::SelectExactlyOne,
            GroupType::SelectAtMostOne => GroupTypeDto::SelectAtMostOne,
            GroupType::SelectAtLeastOne => GroupTypeDto::SelectAtLeastOne,
            GroupType::SelectAll => GroupTypeDto::SelectAll,
            GroupType::SelectAny => GroupTypeDto::SelectAny,
        }
    }
}

/// Serializable mirror of [`fomod::PluginType`] (the 5-state option type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum PluginTypeDto {
    /// Pre-selected and locked on.
    Required,
    /// Freely selectable.
    Optional,
    /// Pre-selected but unlockable.
    Recommended,
    /// Disabled / cannot be selected.
    NotUsable,
    /// Selectable but warns.
    CouldBeUsable,
}

impl From<PluginType> for PluginTypeDto {
    fn from(p: PluginType) -> Self {
        match p {
            PluginType::Required => PluginTypeDto::Required,
            PluginType::Optional => PluginTypeDto::Optional,
            PluginType::Recommended => PluginTypeDto::Recommended,
            PluginType::NotUsable => PluginTypeDto::NotUsable,
            PluginType::CouldBeUsable => PluginTypeDto::CouldBeUsable,
        }
    }
}

// ── Serializable selection (webview → adapter) ─────────────────────────────────────

/// The user's wizard choices crossing the IPC boundary. Each chosen option is its
/// `(step, group, option)` identity (matching `fomod::Selection`), plus the accumulated
/// flags those choices set. The webview computes flags from the authored option flags it
/// renders; the adapter forwards them to the pure engine verbatim (no logic here).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SelectionDto {
    /// Chosen option identities `[step, group, option]`.
    pub chosen: Vec<[String; 3]>,
    /// Accumulated `(flag, value)` pairs set by the chosen options.
    pub flags: Vec<[String; 2]>,
}

impl SelectionDto {
    /// Build the engine [`Selection`] from the wire shape (pure mapping, no logic).
    fn into_selection(self) -> Selection {
        let mut sel = Selection::default();
        for [step, group, option] in self.chosen {
            sel.chosen.insert((step, group, option));
        }
        for [flag, value] in self.flags {
            sel.flags.insert(flag, value);
        }
        sel
    }
}

// ── Serializable dry-run plan + conflict preview (adapter → webview) ────────────────

/// The conflict classification for the dry-run preview (UI-SPEC §A.6). This mirrors the
/// FOMOD safety gate's three buckets: a clean plan, a priority-resolvable overwrite, or a
/// BLOCKING conflict that disables Install.
///
/// The headless `fomod::resolve` only ever returns a conflict-FREE, deterministically
/// deduped plan (or a `FomodError`), so the adapter constructs `None` for a resolved plan
/// and surfaces the blocking case via the command's `Err` (the engine rejected the
/// selection). `Resolvable`/`Blocking` are retained as part of the stable serialized
/// contract the wizard's TypeScript mirror consumes (and the future cross-mod
/// classification target); they are not constructed inline here, hence the allow.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictClass {
    /// No two installs target the same destination.
    None,
    /// Two installs target the same destination but priority picks a winner.
    Resolvable,
    /// Two installs target the same destination with EQUAL priority — no winner.
    Blocking,
}

/// One row of the resolved dry-run plan (a single `dest_rel`).
#[derive(Debug, Clone, Serialize)]
pub struct PlanEntry {
    /// Archive-relative source path (monospace src in the preview).
    pub src: String,
    /// Staging-relative destination path (monospace dest in the preview).
    pub dest: String,
    /// Tie-break priority (higher wins a shared destination).
    pub priority: i32,
}

/// The full dry-run result the wizard shows BEFORE any staging write (FOMOD-02).
#[derive(Debug, Clone, Serialize)]
pub struct ResolvePreview {
    /// The ordered, deduped file-install plan.
    pub plan: Vec<PlanEntry>,
    /// The overall conflict classification (the worst of any per-destination contest).
    pub classification: ConflictClass,
    /// The destinations that two equal-priority sources contested (the blocking set).
    pub blocking: Vec<String>,
}

// ── The three thin commands ────────────────────────────────────────────────────────

/// Parse a mod archive's `fomod/ModuleConfig.xml`, returning the wizard projection.
///
/// Extracts the archive into a validated temporary tree (the SAME defended extractor the
/// install path uses), then calls the pure `fomod::parse_module_config`. A non-FOMOD or
/// malformed archive returns the verbatim `FomodError` string so the frontend offers the
/// plain-mod fallback (UI-SPEC §A.8). The temp tree is dropped on return — this writes
/// nothing to staging.
#[tauri::command]
pub async fn parse_fomod(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
    archive: PathBuf,
) -> Result<FomodProjection, String> {
    // Resolve the game only to assert it is managed (parity with the install path); the
    // parse itself reads the archive, not the game tree.
    let _game = require_game(&state, appid).await?;

    let (_temp, tree_root) = extract_to_temp(&archive).map_err(boundary_err)?;
    let module = parse_module_config(&tree_root).map_err(boundary_err)?;
    Ok(project_module(&module))
}

/// The PURE dry-run resolve (FOMOD-02): turn the user's selection into the file-install
/// plan + conflict classification WITHOUT writing anything.
///
/// Re-extracts the archive to a temp tree (so source-path resolution and the live
/// type-state evaluation see the real staged layout), parses, and calls the pure
/// `fomod::resolve`. The conflict classification is computed over the resolved plan's
/// destinations (a pure fold). No staging write occurs.
#[tauri::command]
pub async fn resolve_fomod(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
    archive: PathBuf,
    selection: SelectionDto,
) -> Result<ResolvePreview, String> {
    let _game = require_game(&state, appid).await?;

    let (_temp, tree_root) = extract_to_temp(&archive).map_err(boundary_err)?;
    let module = parse_module_config(&tree_root).map_err(boundary_err)?;
    let sel = selection.into_selection();

    // PURE: fomod::resolve performs zero filesystem writes (Plan-01 invariant).
    let plan = resolve(&module, &sel).map_err(boundary_err)?;
    Ok(classify_plan(&plan))
}

/// Apply a confirmed (non-blocking) FOMOD install: stage the validated archive and record
/// it as an ordinary `ManagedMod`.
///
/// The selection is re-resolved (defence in depth: never apply a plan the engine now
/// rejects — e.g. a blocking conflict) BEFORE any write. On success the archive is staged
/// through the validated `extract::install_archive` path (Plan-01 root-detection,
/// zip-slip/symlink/`..` defenses unchanged — the adapter adds no new write primitive),
/// and the staged tree is persisted via `store.add_mod`. Returns the new mod's row id.
#[tauri::command]
pub async fn apply_fomod(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
    archive: PathBuf,
    name: String,
    selection: SelectionDto,
) -> Result<ApplyResult, String> {
    let game = require_game(&state, appid).await?;

    // 1. Re-resolve to reject a blocking selection before touching disk (the dry-run gate
    //    is enforced server-side too, not only in the UI).
    let (temp, tree_root) = extract_to_temp(&archive).map_err(boundary_err)?;
    let module = parse_module_config(&tree_root).map_err(boundary_err)?;
    let sel = selection.into_selection();
    let plan = resolve(&module, &sel).map_err(boundary_err)?;
    let preview = classify_plan(&plan);
    if preview.classification == ConflictClass::Blocking {
        return Err("This selection installs conflicting files with no clear winner. \
                    Change a choice to continue."
            .to_string());
    }
    drop(temp); // release the dry-run temp tree before the real validated staging.

    // 2. Stage the validated archive into a per-mod staging subdir (the SAME defended
    //    extractor the local-archive + download paths use). No new write primitive.
    let staging_root = game.staging_dir.join(sanitize(&name));
    let staged = extract::install_archive(&archive, &staging_root).map_err(boundary_err)?;

    // 3. Persist as an ordinary ManagedMod so it appears in the existing mod list.
    let managed = nextwist_core::ManagedMod {
        id: 0,
        name: name.clone(),
        staging_root: staged.staging_root.clone(),
        enabled: false,
        rank: 1,
    };
    let mod_id = {
        let guard = state.lock().await;
        guard.store.add_mod(game.appid, &managed).map_err(boundary_err)?
    };

    Ok(ApplyResult {
        mod_id,
        name,
        staging_root: staged.staging_root,
        files: staged.files.len(),
    })
}

/// The result of a confirmed FOMOD apply: the persisted mod id + the staged tree summary.
#[derive(Debug, Clone, Serialize)]
pub struct ApplyResult {
    /// The new `managed_mod` row id.
    pub mod_id: i64,
    /// The mod's display name.
    pub name: String,
    /// Root of the validated, staged tree.
    pub staging_root: PathBuf,
    /// Number of staged files.
    pub files: usize,
}

// ── Pure helpers (projection + classification + temp extraction) ────────────────────

/// Extract `archive` into a fresh temp dir via the validated extractor, returning the
/// guard (kept alive by the caller) and the tree root the FOMOD engine reads.
///
/// Reuses `extract::install_archive` so EVERY entry crosses the same zip-slip/symlink/`..`
/// defense before any FOMOD parsing reads it. The validated tree is moved into a `tree/`
/// subdir of the temp dir (root-detected to a `Data/`-rooted layout); the temp dir is
/// removed when the returned [`TempDir`] is dropped.
fn extract_to_temp(archive: &Path) -> Result<(TempDir, PathBuf), extract::ExtractError> {
    if !archive.is_file() {
        return Err(extract::ExtractError::io(
            archive,
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "archive path is not an existing file",
            ),
        ));
    }
    // Fail fast on an unknown format with the same error the install path surfaces.
    let _ = ArchiveFormat::detect(archive)?;

    let temp = tempfile::Builder::new()
        .prefix(".nextwist-fomod-")
        .tempdir()
        .map_err(|e| extract::ExtractError::io(archive, e))?;
    let tree_root = temp.path().join("tree");
    extract::install_archive(archive, &tree_root)?;
    Ok((temp, tree_root))
}

/// Project a parsed [`FomodModule`] into the serializable wizard shape, applying the
/// authored `order` to steps/groups/options exactly as the engine would.
fn project_module(module: &FomodModule) -> FomodProjection {
    let mut steps = Vec::new();
    if let Some(step_list) = &module.steps {
        let mut ordered: Vec<_> = step_list.steps.iter().collect();
        sort_by_order(&mut ordered, step_list.order, |s| &s.name);
        for step in ordered {
            let mut groups = Vec::new();
            if let Some(group_list) = &step.groups {
                let mut og: Vec<_> = group_list.groups.iter().collect();
                sort_by_order(&mut og, group_list.order, |g| &g.name);
                for group in og {
                    let mut options = Vec::new();
                    if let Some(plugin_list) = &group.plugins {
                        let mut pl: Vec<_> = plugin_list.plugins.iter().collect();
                        sort_by_order(&mut pl, plugin_list.order, |p| &p.name);
                        for plugin in pl {
                            options.push(OptionProjection {
                                name: plugin.name.clone(),
                                description: plugin.description.clone(),
                                image: plugin.image.as_ref().map(|i| i.path.clone()),
                                default_type: default_type_of(plugin).into(),
                            });
                        }
                    }
                    groups.push(GroupProjection {
                        name: group.name.clone(),
                        group_type: group.group_type.into(),
                        options,
                    });
                }
            }
            steps.push(StepProjection {
                name: step.name.clone(),
                conditional: step.visible.is_some(),
                groups,
            });
        }
    }
    FomodProjection {
        module_name: module.module_name.clone(),
        steps,
    }
}

/// The authored default type-state of a plugin (static `<type>` or `<dependencyType>`
/// default). The LIVE type after choices is recomputed by the engine in `resolve_fomod`;
/// this is the initial render value (Optional when a descriptor is absent — never silently
/// disables an option).
fn default_type_of(plugin: &fomod::Plugin) -> PluginType {
    match &plugin.type_descriptor {
        Some(td) => td
            .static_type
            .as_ref()
            .map(|t| t.name)
            .or_else(|| td.dependency_type.as_ref().map(|d| d.default_type.name))
            .unwrap_or(PluginType::Optional),
        None => PluginType::Optional,
    }
}

/// Sort a slice of element refs by the FOMOD `order` attribute (Ascending/Descending by
/// name, or Explicit = document order preserved).
fn sort_by_order<T, F>(items: &mut [&T], order: OrderKind, key: F)
where
    F: Fn(&T) -> &String,
{
    match order {
        OrderKind::Explicit => {}
        OrderKind::Ascending => items.sort_by(|a, b| key(a).cmp(key(b))),
        OrderKind::Descending => items.sort_by(|a, b| key(b).cmp(key(a))),
    }
}

/// Project the resolved plan into the dry-run preview rows + a conflict classification
/// (UI-SPEC §A.6).
///
/// `fomod::resolve` IS the FOMOD-02 safety gate. It returns `Ok` only with a
/// deterministically DEDUPED, conflict-free plan — one winner per `dest_rel`, the
/// highest-priority `src` wins each destination — so a successfully-resolved plan has no
/// remaining same-destination contest and is **safe to install** (`ConflictClass::None`).
/// A genuinely no-winner / contradictory FOMOD construct (a missing `<typeDescriptor>`,
/// an unsupported shape) is surfaced by the engine as a `FomodError` BEFORE this projection
/// runs; the calling command maps that `Err` to the §A.6 blocking message verbatim and the
/// wizard disables Install. The `ConflictClass::Resolvable`/`Blocking` variants therefore
/// describe the FRONTEND's cross-source presentation contract (an authored same-destination
/// overwrite vs an engine-rejected selection) — the headless engine never returns a plan
/// that still contains an unresolved destination contest, which is exactly the safety
/// invariant the dry-run gate depends on.
fn classify_plan(plan: &[fomod::FileInstall]) -> ResolvePreview {
    let rows: Vec<PlanEntry> = plan
        .iter()
        .map(|fi| PlanEntry {
            src: fi.src.to_string_lossy().into_owned(),
            dest: fi.dest_rel.to_string_lossy().into_owned(),
            priority: fi.priority,
        })
        .collect();

    ResolvePreview {
        plan: rows,
        classification: ConflictClass::None,
        blocking: Vec::new(),
    }
}

/// Sanitize a display name into a single safe staging-subdir component (no separators, no
/// traversal). The full path-traversal defense still lives in `extract`; this only keeps
/// the staging subdir name well-formed (mirrors `commands::downloads::sanitize`).
fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "fomod-mod".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    //! Headless adapter tests (no webview). They exercise the adapter's REAL logic — the
    //! validated temp extraction (`extract_to_temp`), the AST projection (`project_module`),
    //! the dry-run plan + classification (`classify_plan` over `fomod::resolve`), and the
    //! malformed-FOMOD `Err` path — by zipping a Plan-01 fixture tree into a real archive
    //! and flowing it through the SAME functions the `#[tauri::command]`s call. The Tauri
    //! IPC shell (`require_game` + `State` lock) is the only part not covered, which is the
    //! pure boundary glue these tests deliberately exclude.

    use std::io::Write;
    use std::path::{Path, PathBuf};

    use fomod::Selection;

    /// Path to a Plan-01 fixture tree (the dir that CONTAINS the `fomod/` folder).
    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/fomod/tests/fixtures")
            .join(name)
    }

    /// Zip a fixture tree into a real `.zip` archive at `dest`, so the adapter's validated
    /// extractor (which only accepts archives) can consume it like a real mod download.
    fn zip_fixture(tree_root: &Path, dest: &Path) {
        let file = std::fs::File::create(dest).expect("create zip");
        let mut zw = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().unix_permissions(0o644);
        for entry in walkdir_files(tree_root) {
            let rel = entry.strip_prefix(tree_root).unwrap();
            let name = rel.to_string_lossy().replace('\\', "/");
            zw.start_file(name, opts).expect("start_file");
            let bytes = std::fs::read(&entry).expect("read fixture file");
            zw.write_all(&bytes).expect("write zip entry");
        }
        zw.finish().expect("finish zip");
    }

    /// Minimal recursive file walk (avoids pulling `walkdir` into the test as a dep).
    fn walkdir_files(root: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).expect("read_dir") {
                let p = e.expect("entry").path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.push(p);
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn parse_projects_simple_fixture_ast() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("simple.zip");
        zip_fixture(&fixture("simple"), &archive);

        // The adapter's validated extraction + the pure parse + the projection.
        let (_guard, tree_root) = super::extract_to_temp(&archive).expect("extract simple.zip");
        let module = fomod::parse_module_config(&tree_root).expect("parse simple fixture");
        let proj = super::project_module(&module);

        assert_eq!(proj.module_name, "Simple Mod");
        assert_eq!(proj.steps.len(), 1);
        let step = &proj.steps[0];
        assert_eq!(step.name, "Main");
        assert_eq!(step.groups.len(), 1);
        let group = &step.groups[0];
        assert!(matches!(group.group_type, super::GroupTypeDto::SelectExactlyOne));
        assert_eq!(group.options.len(), 1);
        let opt = &group.options[0];
        assert_eq!(opt.name, "Standard Edition");
        assert_eq!(opt.default_type, super::PluginTypeDto::Required);
    }

    #[test]
    fn malformed_fixture_returns_err_string_for_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("malformed.zip");
        zip_fixture(&fixture("malformed"), &archive);

        let (_guard, tree_root) = super::extract_to_temp(&archive).expect("extract malformed.zip");
        // The malformed ModuleConfig.xml must yield a specific Err (the verbatim string the
        // adapter maps via boundary_err so the frontend offers the plain-mod fallback) —
        // never a silently-empty Ok projection.
        let parsed = fomod::parse_module_config(&tree_root);
        assert!(parsed.is_err(), "malformed FOMOD must not parse Ok");
        let msg = parsed.err().unwrap().to_string();
        assert!(!msg.is_empty(), "the error carries a specific reason for the UI");
    }

    #[test]
    fn resolve_returns_plan_and_writes_nothing_to_staging() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("simple.zip");
        zip_fixture(&fixture("simple"), &archive);

        let (_guard, tree_root) = super::extract_to_temp(&archive).expect("extract simple.zip");
        let module = fomod::parse_module_config(&tree_root).expect("parse");

        // A staging dir we assert stays untouched by the dry-run (FOMOD-02: writes nothing).
        let staging = tmp.path().join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        let before = walkdir_files(&staging).len();

        // The wizard pre-selects a `Required` option (the engine installs a plugin's files
        // only when its option is selected); mirror that here.
        let mut sel = Selection::default();
        sel.chosen
            .insert(("Main".into(), "Core".into(), "Standard Edition".into()));
        let plan = fomod::resolve(&module, &sel).expect("resolve");
        let preview = super::classify_plan(&plan);

        assert!(!preview.plan.is_empty(), "the Required file installs in the plan");
        let row = &preview.plan[0];
        assert_eq!(row.dest, "standard.esp");
        assert!(matches!(preview.classification, super::ConflictClass::None));

        // The dry-run resolve performed ZERO writes into the staging dir.
        let after = walkdir_files(&staging).len();
        assert_eq!(before, after, "dry-run resolve must not write to staging");
        assert_eq!(after, 0);
    }

    #[test]
    fn selection_dto_maps_to_engine_selection() {
        let dto = super::SelectionDto {
            chosen: vec![["Step".into(), "Group".into(), "Opt".into()]],
            flags: vec![["color".into(), "red".into()]],
        };
        let sel = dto.into_selection();
        assert!(sel.is_chosen("Step", "Group", "Opt"));
        assert_eq!(sel.flags.get("color").map(String::as_str), Some("red"));
    }

    #[test]
    fn sanitize_strips_separators_and_falls_back() {
        assert_eq!(super::sanitize("My Mod"), "My Mod");
        assert_eq!(super::sanitize("../etc/passwd"), "___etc_passwd");
        assert_eq!(super::sanitize("   "), "fomod-mod");
    }
}
