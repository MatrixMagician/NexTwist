//! The PURE dry-run resolver: (module + selection) → ordered file-install plan.
//!
//! INVARIANT: [`resolve`] is a pure function — it performs ZERO filesystem writes. This
//! is the locked "dry-run-resolve-then-apply" safety gate; the returned plan is surfaced
//! (and conflict-previewed) before the validated extract→staging path applies it. The
//! purity is unit-tested (a no-write marker test runs resolve and asserts a temp dir
//! stays empty).
//!
//! Order (per the FOMOD spec + the plan's behavior block):
//! 1. `requiredInstallFiles` — unconditional.
//! 2. Each selected plugin's files — plus `alwaysInstall` items (even unselected) and
//!    `installIfUsable` items when the owning option is not `NotUsable`.
//! 3. `conditionalFileInstalls` patterns whose dependencies hold (against `sel.flags`).
//!
//! Then dedup by `(dest_rel, priority desc)` so the highest-priority `src` wins a
//! destination (a stable, deterministic last-writer-by-priority fold).

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::condition::{eval, plugin_type_state, FlagSet, InstalledFiles};
use crate::error::FomodError;
use crate::model::{FileItem, FileList, FomodModule, PluginType};

/// One concrete file install in the resolved plan.
///
/// `src` is the archive-relative `source`; `dest_rel` is the staging-relative destination
/// (a `<file>`/`<folder>` with no `destination` lands at the `Data/` root). `priority`
/// breaks ties when two installs target the same destination (higher wins).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInstall {
    /// Archive-relative source path.
    pub src: PathBuf,
    /// Staging-relative destination path.
    pub dest_rel: PathBuf,
    /// Tie-break priority (higher wins a shared destination).
    pub priority: i32,
    /// Whether this install came from an `alwaysInstall` item.
    pub always: bool,
}

/// The user's (or replayed) selection: the set of chosen option names per group, plus the
/// accumulated flag set and any known installed-file state.
///
/// Selected options are identified by `(step_name, group_name, option_name)` so a
/// headless Collection replay (`IChoices`) maps onto the same `Selection` the interactive
/// wizard builds.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// The chosen option identities `(step, group, option)`.
    pub chosen: HashSet<(String, String, String)>,
    /// The accumulated flags set by the chosen options.
    pub flags: FlagSet,
    /// Known installed-file state for `fileDependency` evaluation (empty for a pure
    /// dry-run with no live game state).
    pub files: InstalledFiles,
}

impl Selection {
    /// Whether the option `(step, group, option)` is selected.
    pub fn is_chosen(&self, step: &str, group: &str, option: &str) -> bool {
        self.chosen
            .contains(&(step.to_string(), group.to_string(), option.to_string()))
    }
}

/// Resolve the concrete, ordered file-install plan for `module` under `selection`.
///
/// PURE — never writes to disk. See the module docs for the ordering + dedup contract.
pub fn resolve(module: &FomodModule, selection: &Selection) -> Result<Vec<FileInstall>, FomodError> {
    let mut plan: Vec<FileInstall> = Vec::new();

    // 1. requiredInstallFiles — unconditional.
    if let Some(req) = &module.required {
        append_file_list(req, &mut plan);
    }

    // 2. Per-plugin files. Walk every step/group/plugin; a plugin's files install when
    //    the option is selected, OR an item is alwaysInstall, OR an item is
    //    installIfUsable and the option's live type is not NotUsable.
    if let Some(steps) = &module.steps {
        for step in &steps.steps {
            // A step with a `<visible>` dependency contributes its files ONLY when that
            // dependency holds against the current flags/files (FOMOD spec step visibility,
            // WR-01). An invisible step is skipped entirely — its selected/Required options
            // must NOT reach the install plan. A step with no `<visible>` is always visible.
            if let Some(vis) = &step.visible
                && !eval(vis, &selection.flags, &selection.files)
            {
                continue;
            }
            let Some(groups) = &step.groups else { continue };
            for group in &groups.groups {
                let Some(plugins) = &group.plugins else {
                    continue;
                };
                for plugin in &plugins.plugins {
                    let Some(files) = &plugin.files else { continue };

                    let selected = selection.is_chosen(&step.name, &group.name, &plugin.name);

                    // Live plugin type-state (drives installIfUsable + a malformed check).
                    let plugin_type = match &plugin.type_descriptor {
                        Some(td) => plugin_type_state(td, &selection.flags, &selection.files),
                        None => {
                            return Err(FomodError::MalformedSchema(format!(
                                "plugin '{}' in group '{}' has no <typeDescriptor>",
                                plugin.name, group.name
                            )));
                        }
                    };
                    let usable = plugin_type != PluginType::NotUsable;

                    append_plugin_files(files, selected, usable, &mut plan);
                }
            }
        }
    }

    // 3. conditionalFileInstalls — every pattern whose dependencies hold.
    if let Some(list) = module.conditional.as_ref().and_then(|c| c.patterns.as_ref()) {
        for pattern in &list.patterns {
            let holds = pattern
                .dependencies
                .as_ref()
                .map(|d| eval(d, &selection.flags, &selection.files))
                // A pattern with no <dependencies> always installs.
                .unwrap_or(true);
            if holds && let Some(files) = &pattern.files {
                append_file_list(files, &mut plan);
            }
        }
    }

    Ok(dedup_by_priority(plan))
}

/// Append every `<file>`/`<folder>` in `list` to the plan (unconditional context — used
/// for `requiredInstallFiles` and conditional-pattern file lists).
fn append_file_list(list: &FileList, plan: &mut Vec<FileInstall>) {
    for item in list.files.iter().chain(list.folders.iter()) {
        plan.push(file_install(item));
    }
}

/// Append a selected plugin's files honoring `alwaysInstall` / `installIfUsable`.
///
/// * `selected` — the option is chosen.
/// * `usable` — the option's live type is not `NotUsable`.
fn append_plugin_files(
    list: &FileList,
    selected: bool,
    usable: bool,
    plan: &mut Vec<FileInstall>,
) {
    for item in list.files.iter().chain(list.folders.iter()) {
        let install = selected
            || item.always_install
            || (item.install_if_usable && usable);
        if install {
            plan.push(file_install(item));
        }
    }
}

/// Build a [`FileInstall`] from a model [`FileItem`]. An absent `destination` lands at the
/// `Data/` root, modeled as the relative path equal to the source's file name component
/// (the staging tree itself is the `Data/`-rooted root after archive-root detection).
fn file_install(item: &FileItem) -> FileInstall {
    let src = PathBuf::from(item.source.replace('\\', "/"));
    let dest_rel = match &item.destination {
        Some(dest) if !dest.is_empty() => PathBuf::from(dest.replace('\\', "/")),
        // Absent/empty destination ⇒ Data root: keep the source's leaf (and any
        // sub-path relative to the source's own root is preserved by callers via the
        // folder expansion at apply time; here a bare file maps to its file name).
        _ => data_root_dest(&src),
    };
    FileInstall {
        src,
        dest_rel,
        priority: item.priority,
        always: item.always_install,
    }
}

/// Destination for a `destination`-less item: the `Data/` root holds the source's leaf.
fn data_root_dest(src: &Path) -> PathBuf {
    src.file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| src.to_path_buf())
}

/// Dedup the plan by destination, keeping the highest-priority source for each `dest_rel`.
///
/// Deterministic: for a given destination the surviving entry is the one with the greatest
/// `priority`; ties keep the FIRST-seen entry (install order). The output is sorted by
/// `dest_rel` so the plan is stable regardless of discovery order.
fn dedup_by_priority(plan: Vec<FileInstall>) -> Vec<FileInstall> {
    let mut winners: HashMap<PathBuf, FileInstall> = HashMap::new();
    for fi in plan {
        match winners.get(&fi.dest_rel) {
            Some(existing) if existing.priority >= fi.priority => { /* keep existing */ }
            _ => {
                winners.insert(fi.dest_rel.clone(), fi);
            }
        }
    }
    let mut out: Vec<FileInstall> = winners.into_values().collect();
    out.sort_by(|a, b| a.dest_rel.cmp(&b.dest_rel));
    out
}
