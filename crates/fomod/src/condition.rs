//! The composite-dependency / flag evaluator + live plugin type-state resolver.
//!
//! STUB (Task 1 RED): signatures only. Implemented in Task 2 (GREEN).

use std::collections::HashMap;

use crate::model::{CompositeDependency, FileState, PluginType, TypeDescriptor};

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
/// `And` ⇒ all arms hold; `Or` ⇒ any arm holds. Empty `And` ⇒ true, empty `Or` ⇒ false.
pub fn eval(dep: &CompositeDependency, flags: &FlagSet, files: &InstalledFiles) -> bool {
    let _ = (dep, flags, files);
    unimplemented!("eval implemented in Task 2 (GREEN)")
}

/// Resolve the live plugin type for a [`TypeDescriptor`] given the current `flags`/`files`:
/// a static `<type>` returns directly; a `<dependencyType>` walks its patterns in order and
/// returns the first whose dependencies hold, else `defaultType`.
pub fn plugin_type_state(
    descriptor: &TypeDescriptor,
    flags: &FlagSet,
    files: &InstalledFiles,
) -> PluginType {
    let _ = (descriptor, flags, files);
    unimplemented!("plugin_type_state implemented in Task 2 (GREEN)")
}
