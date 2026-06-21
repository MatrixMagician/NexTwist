//! The PURE dry-run resolver: (module + selection) → ordered file-install plan.
//!
//! STUB (Task 1 RED): signatures only. Implemented in Task 2 (GREEN).
//!
//! INVARIANT: [`resolve`] is a pure function — it performs ZERO filesystem writes. This
//! is the locked "dry-run-resolve-then-apply" safety gate; the returned plan is surfaced
//! (and conflict-previewed) before the validated extract→staging path applies it. The
//! purity is unit-tested (a no-write marker test runs resolve against a temp dir and
//! asserts it stays empty).

use std::collections::HashSet;
use std::path::PathBuf;

use crate::condition::{FlagSet, InstalledFiles};
use crate::error::FomodError;
use crate::model::FomodModule;

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
/// PURE — never writes to disk. Order: `requiredInstallFiles` (unconditional) → each
/// selected plugin's files (or `alwaysInstall`, or `installIfUsable` when the owning
/// option is not `NotUsable`) → `conditionalFileInstalls` patterns whose dependencies
/// hold; then dedup by `(dest_rel, priority desc)` so the highest-priority `src` wins a
/// destination. A genuinely unsupported construct returns a specific [`FomodError`].
pub fn resolve(module: &FomodModule, selection: &Selection) -> Result<Vec<FileInstall>, FomodError> {
    let _ = (module, selection);
    unimplemented!("resolve implemented in Task 2 (GREEN)")
}
