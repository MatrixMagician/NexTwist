//! The composite-dependency / flag evaluator + live plugin type-state resolver.
//!
//! `eval` is a pure boolean tree-walk over a [`CompositeDependency`]: it reads the
//! accumulated flag set and the (optional) installed-file state and returns whether the
//! dependency holds. It executes NO code — the name `eval` denotes FOMOD condition
//! evaluation, not interpretation of any expression language.

use std::collections::HashMap;

use crate::model::{CompositeDependency, FileState, Operator, PluginType, TypeDescriptor};

/// The accumulated flag set: flag name → current value (set by selected options).
pub type FlagSet = HashMap<String, String>;

/// The installed-file state oracle for `fileDependency` evaluation.
///
/// FOMOD `fileDependency` checks whether a game/plugin file is `Missing`/`Inactive`/
/// `Active`. During a pure dry-run resolve the engine has no live game state, so the
/// default treats every queried file as `Missing` — callers that DO have state supply it.
#[derive(Debug, Clone, Default)]
pub struct InstalledFiles {
    /// Per-file known states; absent files are treated as `Missing`.
    pub states: HashMap<String, FileState>,
}

impl InstalledFiles {
    /// The known state of `file`, defaulting to [`FileState::Missing`].
    pub fn state(&self, file: &str) -> FileState {
        self.states.get(file).copied().unwrap_or(FileState::Missing)
    }
}

/// Recursively evaluate a [`CompositeDependency`] against `flags` + `files`.
///
/// `And` ⇒ all arms hold; `Or` ⇒ any arm holds. Empty `And` ⇒ true, empty `Or` ⇒ false
/// (the natural identity for each operator). Game/fomm version arms are treated as
/// satisfied during the headless dry-run (no live game-version oracle here); they exist
/// in the AST and can be wired to a real version once the apply path supplies one.
pub fn eval(dep: &CompositeDependency, flags: &FlagSet, files: &InstalledFiles) -> bool {
    // Collect each arm's truth value lazily so And short-circuits on the first false and
    // Or on the first true.
    let flag_arms = dep
        .flag_deps
        .iter()
        .map(|f| flags.get(&f.flag).is_some_and(|v| v == &f.value));

    let file_arms = dep
        .file_deps
        .iter()
        .map(|f| files.state(&f.file) == f.state);

    // Version dependencies: no live game version in the pure dry-run ⇒ treated as held.
    // (Documented behavior; the apply path can supply a real comparator later.)
    let version_arms = std::iter::repeat_n(true, dep.game_deps.len() + dep.fomm_deps.len());

    let nested_arms = dep.nested.iter().map(|inner| eval(inner, flags, files));

    let mut results = flag_arms
        .chain(file_arms)
        .chain(version_arms)
        .chain(nested_arms);

    match dep.operator {
        Operator::And => results.all(|r| r),
        Operator::Or => results.any(|r| r),
    }
}

/// Resolve the live plugin type for a [`TypeDescriptor`] given the current `flags`/`files`:
/// a static `<type>` returns directly; a `<dependencyType>` walks its patterns in order and
/// returns the first whose dependencies hold, else `defaultType`.
pub fn plugin_type_state(
    descriptor: &TypeDescriptor,
    flags: &FlagSet,
    files: &InstalledFiles,
) -> PluginType {
    if let Some(static_type) = &descriptor.static_type {
        return static_type.name;
    }
    if let Some(dt) = &descriptor.dependency_type {
        if let Some(list) = &dt.patterns {
            for pattern in &list.patterns {
                let holds = pattern
                    .dependencies
                    .as_ref()
                    .map(|d| eval(d, flags, files))
                    // A pattern with no <dependencies> always applies.
                    .unwrap_or(true);
                if holds {
                    return pattern.plugin_type.name;
                }
            }
        }
        return dt.default_type.name;
    }
    // Neither a static nor a dependency type present — default to Optional so a partially
    // specified descriptor never silently disables an option.
    PluginType::Optional
}
