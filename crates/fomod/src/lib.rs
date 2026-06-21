//! `nextwist-fomod` — the headless FOMOD `ModuleConfig.xml` engine.
//!
//! This crate is the only genuinely new safety-critical engine code in Phase 4. It
//! implements the FULL FOMOD 5.x `ModuleConfig.xml` specification (CONTEXT-locked
//! "full spec, not a subset" decision) as a pure transform: untrusted archive XML →
//! typed AST → (user choices + accumulated flags) → an ordered, concrete file-install
//! plan. It is **Tauri-free, reqwest-free, and keyring-free** by design — the wizard UI
//! and all OS-integration live in the `src-tauri` shell, which calls into this crate.
//!
//! ## The three-stage split (parse → condition → resolve)
//!
//! * [`parse`] — locate `fomod/ModuleConfig.xml` case-insensitively in the extracted
//!   tree, strip a UTF-8 BOM, and `quick_xml::de::from_str` it into the [`model`] AST.
//!   Deserialization is namespace-ignorant (matches local element names, ignores the
//!   `xsi:noNamespaceSchemaLocation` noise) and treats every optional element as
//!   absent-by-default.
//! * [`condition`] — the recursive composite-dependency evaluator (`And`/`Or`, nested,
//!   `fileDependency`/`flagDependency`/`gameDependency`) over an accumulated flag set,
//!   plus the live plugin type-state resolver (walk `dependencyType.patterns` in order).
//! * [`resolve`] — the **PURE dry-run** entry point: given a [`resolve::Selection`] it
//!   returns an ordered `Vec<`[`resolve::FileInstall`]`>` (the file-install plan)
//!   WITHOUT writing anything to disk. This is the locked "dry-run-resolve-then-apply"
//!   safety gate — the plan is surfaced (and conflict-previewed) before the validated
//!   extract→staging path applies it. The pure-ness is itself unit-tested.
//!
//! A genuinely malformed or unsupported construct returns a specific [`error::FomodError`]
//! ([`error::FomodError::Xml`] or [`error::FomodError::MalformedSchema`]) — never a
//! silent mis-install (the locked "fail clearly, never mis-install" requirement).

pub mod condition;
pub mod error;
pub mod model;
pub mod parse;
pub mod resolve;

pub use condition::{eval, plugin_type_state, FlagSet, InstalledFiles};
pub use error::FomodError;
pub use model::{
    CompositeDependency, ConditionalFileInstalls, Dependency, DependencyType, FileItem, FileList,
    FomodModule, Group, GroupType, InstallStep, Operator, OrderKind, Pattern, Plugin, PluginType,
    TypeDescriptor,
};
pub use parse::{parse_module_config, resolve_source_path};
pub use resolve::{resolve, FileInstall, Selection};
