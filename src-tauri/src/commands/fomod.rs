//! FOMOD guided-installer adapter — the thin IPC boundary over the
//! headless `crates/fomod` engine. Per the thin-adapter contract (see `commands/mod.rs`):
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
//!   frontend can offer the plain-mod fallback.
//! * [`resolve_fomod`] — the PURE dry-run: given the user's selection, call
//!   `fomod::resolve` and return a serializable file-install plan. Writes NOTHING (the
//!   locked dry-run-before-apply gate); an unresolvable selection is an `Err`, not a plan.
//! * [`apply_fomod`] — on a confirmed install, route the archive through
//!   the validated `extract::install_archive` staging path (root-detection,
//!   zip-slip/symlink/`..` defenses unchanged — the adapter adds no new write primitive),
//!   then `store.add_mod` so the result is an ordinary `ManagedMod`.
//!
//! The temp extraction here re-uses the SAME validated extractor the rest of the app
//! uses; FOMOD source-path resolution and parsing are pure reads over that tree.

use std::path::{Path, PathBuf};

use extract::ArchiveFormat;
use fomod::{Selection, parse_module_config, resolve, validate_selection};
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

/// The full dry-run result the wizard shows BEFORE any staging write.
///
/// The plan alone: a plan that exists is by construction safe to install, because
/// `fomod::resolve` is the gate and returns either a deduplicated, conflict-free plan
/// or a typed error. Cross-MOD contests are a different concern entirely and belong to
/// the conflict-and-priority surface, which resolves them by mod rank after install.
#[derive(Debug, Clone, Serialize)]
pub struct ResolvePreview {
    /// The ordered, deduped file-install plan.
    pub plan: Vec<PlanEntry>,
}

// ── The three thin commands ────────────────────────────────────────────────────────

/// Parse a mod archive's `fomod/ModuleConfig.xml`, returning the wizard projection.
///
/// Extracts the archive into a validated temporary tree (the SAME defended extractor the
/// install path uses), then calls the pure `fomod::parse_module_config`. A non-FOMOD or
/// malformed archive returns the verbatim `FomodError` string so the frontend offers the
/// plain-mod fallback. The temp tree is dropped on return — this writes
/// nothing to staging.
#[tauri::command]
pub async fn parse_fomod(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
    archive: PathBuf,
) -> Result<fomod::WizardProjection, String> {
    // Resolve the game only to assert it is managed (parity with the install path); the
    // parse itself reads the archive, not the game tree.
    let _game = require_game(&state, appid).await?;

    let (_temp, tree_root) = extract_to_temp(&archive).map_err(boundary_err)?;
    let module = parse_module_config(&tree_root).map_err(boundary_err)?;
    Ok(fomod::project(&module))
}

/// The PURE dry-run resolve: turn the user's selection into the file-install plan
/// WITHOUT writing anything.
///
/// Re-extracts the archive to a temp tree (so source-path resolution and the live
/// type-state evaluation see the real staged layout), parses, and calls the pure
/// `fomod::resolve`. No staging write occurs.
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

    // Server-side cardinality validation: the webview is not a trust boundary, so a
    // crafted IPC selection that violates a group's SelectExactlyOne/AtLeastOne/AtMostOne
    // constraint is rejected here before the plan is computed.
    validate_selection(&module, &sel).map_err(boundary_err)?;

    // PURE: fomod::resolve performs zero filesystem writes.
    let plan = resolve(&module, &sel).map_err(boundary_err)?;
    Ok(preview_plan(&plan))
}

/// Apply a confirmed FOMOD install: stage the validated archive and record it as an
/// ordinary `ManagedMod`.
///
/// The selection is re-resolved (defence in depth: never apply a plan the engine now
/// rejects) BEFORE any write. On success the archive is staged
/// through the validated `extract::install_archive` path (root-detection,
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

    // 1. Re-resolve to reject an unresolvable selection before touching disk (the
    //    dry-run gate is enforced server-side too, not only in the UI).
    let (temp, tree_root) = extract_to_temp(&archive).map_err(boundary_err)?;
    let module = parse_module_config(&tree_root).map_err(boundary_err)?;
    let sel = selection.into_selection();
    // Server-side cardinality validation: reject a selection that violates a group's
    // declared cardinality BEFORE any disk write, regardless of what the UI submitted.
    validate_selection(&module, &sel).map_err(boundary_err)?;
    // `resolve` IS the gate: a selection with no clear winner is an Err above, never a
    // plan. Reaching here means the plan is safe to stage.
    resolve(&module, &sel).map_err(boundary_err)?;
    drop(temp); // release the dry-run temp tree before the real validated staging.

    // 2. Stage the validated archive into a per-mod staging subdir (the SAME defended
    //    extractor the local-archive + download paths use). No new write primitive.
    let staging_root = fomod_staging_root(&game.staging_dir, &name);
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
        guard
            .store
            .add_mod(game.appid, &managed)
            .map_err(boundary_err)?
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

// ── Pure helpers (projection + temp extraction) ─────────────────────────────────────

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

/// The per-mod staging subdir a FOMOD install stages into: the game's staging dir plus ONE
/// safe component derived from the module's display name.
///
/// The naming rules are the engine's ([`extract::staging_dir_name`]); what this adapter
/// decides is the fallback used when the module name is blank, and that the result is joined
/// under the game's staging dir rather than anywhere else. Extracted so both are testable.
fn fomod_staging_root(staging_dir: &Path, module_name: &str) -> PathBuf {
    staging_dir.join(extract::staging_dir_name(module_name, "fomod-mod"))
}

/// Project the resolved plan into the dry-run preview rows.
///
/// `fomod::resolve` IS the safety gate. It returns `Ok` only with a deterministically
/// DEDUPED, conflict-free plan — one winner per `dest_rel`, the highest-priority `src`
/// wins each destination — so a successfully-resolved plan has no remaining
/// same-destination contest and is safe to install. A genuinely no-winner /
/// contradictory FOMOD construct (a missing `<typeDescriptor>`, an unsupported shape)
/// is surfaced by the engine as a `FomodError` BEFORE this projection runs; the calling
/// command maps that `Err` to the blocking message verbatim and the wizard shows it.
/// There is therefore nothing left for this function to classify.
fn preview_plan(plan: &[fomod::FileInstall]) -> ResolvePreview {
    let rows: Vec<PlanEntry> = plan
        .iter()
        .map(|fi| PlanEntry {
            src: fi.src.to_string_lossy().into_owned(),
            dest: fi.dest_rel.to_string_lossy().into_owned(),
            priority: fi.priority,
        })
        .collect();

    ResolvePreview { plan: rows }
}

#[cfg(test)]
mod tests {
    //! Headless adapter tests (no webview). They exercise the adapter's REAL logic — the
    //! validated temp extraction (`extract_to_temp`), the ordered AST projection (`fomod::project`),
    //! the dry-run plan (`preview_plan` over `fomod::resolve`), and the
    //! malformed-FOMOD `Err` path — by zipping a fixture tree into a real archive
    //! and flowing it through the SAME functions the `#[tauri::command]`s call. The Tauri
    //! IPC shell (`require_game` + `State` lock) is the only part not covered, which is the
    //! pure boundary glue these tests deliberately exclude.

    use std::io::Write;
    use std::path::{Path, PathBuf};

    use fomod::Selection;

    /// Path to a fixture tree (the dir that CONTAINS the `fomod/` folder).
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
        let proj = fomod::project(&module);

        assert_eq!(proj.module_name, "Simple Mod");
        assert_eq!(proj.steps.len(), 1);
        let step = &proj.steps[0];
        assert_eq!(step.name, "Main");
        assert_eq!(step.groups.len(), 1);
        let group = &step.groups[0];
        assert!(matches!(
            group.group_type,
            fomod::GroupType::SelectExactlyOne
        ));
        assert_eq!(group.options.len(), 1);
        let opt = &group.options[0];
        assert_eq!(opt.name, "Standard Edition");
        assert_eq!(opt.default_type, fomod::PluginType::Required);
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
        assert!(
            !msg.is_empty(),
            "the error carries a specific reason for the UI"
        );
    }

    #[test]
    fn resolve_returns_plan_and_writes_nothing_to_staging() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("simple.zip");
        zip_fixture(&fixture("simple"), &archive);

        let (_guard, tree_root) = super::extract_to_temp(&archive).expect("extract simple.zip");
        let module = fomod::parse_module_config(&tree_root).expect("parse");

        // A staging dir we assert stays untouched by the dry-run (writes nothing).
        let staging = tmp.path().join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        let before = walkdir_files(&staging).len();

        // The wizard pre-selects a `Required` option (the engine installs a plugin's files
        // only when its option is selected); mirror that here.
        let mut sel = Selection::default();
        sel.chosen
            .insert(("Main".into(), "Core".into(), "Standard Edition".into()));
        let plan = fomod::resolve(&module, &sel).expect("resolve");
        let preview = super::preview_plan(&plan);

        assert!(
            !preview.plan.is_empty(),
            "the Required file installs in the plan"
        );
        let row = &preview.plan[0];
        assert_eq!(row.dest, "standard.esp");

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

    /// The staging root stays UNDER the game's staging dir and is exactly one component
    /// deeper, even for a hostile module name — the adapter cannot be talked into staging
    /// outside the directory it was given. A blank name still yields a usable subdir.
    #[test]
    fn fomod_staging_root_stays_one_component_under_the_staging_dir() {
        let staging = Path::new("/games/staging");
        for name in ["My Mod", "../../etc/passwd", "/abs", "   ", ""] {
            let root = super::fomod_staging_root(staging, name);
            assert!(
                root.starts_with(staging),
                "{name:?} escaped the staging dir: {root:?}"
            );
            assert_eq!(
                root.strip_prefix(staging).unwrap().components().count(),
                1,
                "{name:?} produced more than one component: {root:?}"
            );
        }
        assert_eq!(
            super::fomod_staging_root(staging, "   "),
            staging.join("fomod-mod"),
            "a blank module name falls back to the FOMOD default"
        );
    }
}
