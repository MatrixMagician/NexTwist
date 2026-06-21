//! The typed FOMOD 5.x AST, serde-derived directly from the canonical
//! `GandaG/fomod-schema/ModuleConfig.xsd` element tree.
//!
//! Every type maps to a LOCAL element name (`#[serde(rename = ...)]`) because quick-xml's
//! serde is namespace-ignorant by default — we match `config`, not `{ns}config`, so the
//! `xsi:noNamespaceSchemaLocation` attribute on real-world files is ignored (Pitfall 5).
//! Every OPTIONAL element/attribute carries `#[serde(default)]` so a legitimately-absent
//! element deserializes as empty rather than erroring (the XSD marks `moduleDependencies`,
//! `requiredInstallFiles`, `installSteps`, `conditionalFileInstalls` and most leaf
//! attributes as optional).
//!
//! This module is pure data — no logic. [`crate::condition`] evaluates the dependency
//! tree and [`crate::resolve`] folds the file lists into an install plan.

use serde::Deserialize;

/// Root `<config>` element (XSD type `moduleConfiguration`).
///
/// Children, in XSD document order: `moduleName`, `moduleImage?`, `moduleDependencies?`,
/// `requiredInstallFiles?`, `installSteps?`, `conditionalFileInstalls?`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename = "config")]
pub struct FomodModule {
    /// The module title (`<moduleName>`). Text content; presentation attributes ignored.
    #[serde(rename = "moduleName", default)]
    pub module_name: String,

    /// `<moduleDependencies>` — gates whether the module is installable at all.
    #[serde(rename = "moduleDependencies", default)]
    pub module_deps: Option<CompositeDependency>,

    /// `<requiredInstallFiles>` — files installed UNCONDITIONALLY.
    #[serde(rename = "requiredInstallFiles", default)]
    pub required: Option<FileList>,

    /// `<installSteps>` — the ordered wizard pages.
    #[serde(rename = "installSteps", default)]
    pub steps: Option<StepList>,

    /// `<conditionalFileInstalls>` — the post-choice pattern engine.
    #[serde(rename = "conditionalFileInstalls", default)]
    pub conditional: Option<ConditionalFileInstalls>,
}

/// `<installSteps order="...">` wrapping `<installStep>` children.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct StepList {
    /// Step ordering (`Ascending` default | `Descending` | `Explicit`).
    #[serde(rename = "@order", default)]
    pub order: OrderKind,
    /// The wizard steps.
    #[serde(rename = "installStep", default)]
    pub steps: Vec<InstallStep>,
}

/// `<installStep name="..." >`: one wizard page.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct InstallStep {
    /// Step name (required attribute).
    #[serde(rename = "@name", default)]
    pub name: String,
    /// `<visible>` — the step is shown only if these deps hold (optional).
    #[serde(rename = "visible", default)]
    pub visible: Option<CompositeDependency>,
    /// `<optionalFileGroups>` — the groups of selectable options.
    #[serde(rename = "optionalFileGroups", default)]
    pub groups: Option<GroupList>,
}

/// `<optionalFileGroups order="...">` wrapping `<group>` children.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GroupList {
    /// Group ordering within the step.
    #[serde(rename = "@order", default)]
    pub order: OrderKind,
    /// The option groups.
    #[serde(rename = "group", default)]
    pub groups: Vec<Group>,
}

/// `<group name="..." type="...">`: a selection group of plugins.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Group {
    /// Group name (required attribute).
    #[serde(rename = "@name", default)]
    pub name: String,
    /// The selection constraint (required attribute).
    #[serde(rename = "@type")]
    pub group_type: GroupType,
    /// `<plugins order="...">` wrapping `<plugin>` children.
    #[serde(rename = "plugins", default)]
    pub plugins: Option<PluginList>,
}

/// The 5 FOMOD selection-group types (XSD `groupType` enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum GroupType {
    /// Exactly one option must be selected (radio).
    SelectExactlyOne,
    /// At most one option may be selected (radio, none allowed).
    SelectAtMostOne,
    /// At least one option must be selected (checkbox, min 1).
    SelectAtLeastOne,
    /// All options are selected (checkbox, all locked on).
    SelectAll,
    /// Any number of options may be selected (checkbox, free).
    SelectAny,
}

/// `<plugins order="...">` wrapping `<plugin>` children.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PluginList {
    /// Plugin ordering within the group.
    #[serde(rename = "@order", default)]
    pub order: OrderKind,
    /// The selectable options.
    #[serde(rename = "plugin", default)]
    pub plugins: Vec<Plugin>,
}

/// `<plugin name="...">`: one selectable option.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Plugin {
    /// Option name (required attribute).
    #[serde(rename = "@name", default)]
    pub name: String,
    /// `<description>` text (optional).
    #[serde(rename = "description", default)]
    pub description: String,
    /// `<image path="...">` (optional).
    #[serde(rename = "image", default)]
    pub image: Option<Image>,
    /// `<files>` installed when this option is selected (optional).
    #[serde(rename = "files", default)]
    pub files: Option<FileList>,
    /// `<conditionFlags>` set when this option is selected (optional).
    #[serde(rename = "conditionFlags", default)]
    pub condition_flags: Option<ConditionFlags>,
    /// `<typeDescriptor>` — the option's type (static or conditional). Required by the
    /// XSD; modeled as `Option` so a malformed file surfaces as a specific error in
    /// resolve rather than failing the whole parse.
    #[serde(rename = "typeDescriptor", default)]
    pub type_descriptor: Option<TypeDescriptor>,
}

/// `<image path="...">`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Image {
    /// Archive-relative image path.
    #[serde(rename = "@path", default)]
    pub path: String,
}

/// `<conditionFlags>` → `<flag name="...">value</flag>`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct ConditionFlags {
    /// The flags this option sets when selected.
    #[serde(rename = "flag", default)]
    pub flags: Vec<SetFlag>,
}

/// `<flag name="...">value</flag>` — a flag a selected option sets.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SetFlag {
    /// Flag name.
    #[serde(rename = "@name", default)]
    pub name: String,
    /// Flag value (the element's text content).
    #[serde(rename = "$text", default)]
    pub value: String,
}

/// `<typeDescriptor>`: EITHER a static `<type>` OR a conditional `<dependencyType>`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TypeDescriptor {
    /// A static `<type name="...">`.
    #[serde(rename = "type", default)]
    pub static_type: Option<PluginTypeElem>,
    /// A conditional `<dependencyType>`.
    #[serde(rename = "dependencyType", default)]
    pub dependency_type: Option<DependencyType>,
}

/// `<type name="...">` — a static plugin type.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PluginTypeElem {
    /// The plugin type.
    #[serde(rename = "@name")]
    pub name: PluginType,
}

/// `<dependencyType>`: `<defaultType>` + ordered `<patterns>`.
///
/// The plugin's live type is the first pattern whose dependencies hold, else
/// `default_type`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DependencyType {
    /// `<defaultType name="...">`.
    #[serde(rename = "defaultType")]
    pub default_type: PluginTypeElem,
    /// `<patterns>` → ordered `<pattern>` (`<dependencies>` + `<type>`).
    #[serde(rename = "patterns", default)]
    pub patterns: Option<TypePatternList>,
}

/// `<patterns>` of `<pattern>` (`<dependencies>` + `<type>`) for a conditional type.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TypePatternList {
    /// Ordered type patterns.
    #[serde(rename = "pattern", default)]
    pub patterns: Vec<TypePattern>,
}

/// `<pattern>` inside a `<dependencyType>`: deps → a plugin type.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TypePattern {
    /// The dependencies that must hold for this pattern's type to apply.
    #[serde(rename = "dependencies", default)]
    pub dependencies: Option<CompositeDependency>,
    /// The plugin type applied when the dependencies hold.
    #[serde(rename = "type")]
    pub plugin_type: PluginTypeElem,
}

/// The 5-state FOMOD plugin-type enum (XSD `pluginTypeEnum`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum PluginType {
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

/// `<files>` / `<requiredInstallFiles>`: a list of `<file>` and `<folder>` items.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct FileList {
    /// `<file>` items.
    #[serde(rename = "file", default)]
    pub files: Vec<FileItem>,
    /// `<folder>` items (expanded to their file tree at resolve time).
    #[serde(rename = "folder", default)]
    pub folders: Vec<FileItem>,
}

/// A `<file>` or `<folder>` (XSD `fileSystemItem`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FileItem {
    /// `source` — archive-relative path (required attribute).
    #[serde(rename = "@source", default)]
    pub source: String,
    /// `destination` — absent ⇒ install to the `Data/` root.
    #[serde(rename = "@destination", default)]
    pub destination: Option<String>,
    /// `priority` — higher wins when two installs target the same destination.
    #[serde(rename = "@priority", default)]
    pub priority: i32,
    /// `alwaysInstall` — install even when the owning option is unselected.
    #[serde(rename = "@alwaysInstall", default)]
    pub always_install: bool,
    /// `installIfUsable` — install when the owning option is not `NotUsable`, even if
    /// unselected.
    #[serde(rename = "@installIfUsable", default)]
    pub install_if_usable: bool,
}

/// `<conditionalFileInstalls>` → `<patterns>` → `<pattern>` (deps + files).
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct ConditionalFileInstalls {
    /// `<patterns>` wrapping the conditional install patterns.
    #[serde(rename = "patterns", default)]
    pub patterns: Option<PatternList>,
}

/// `<patterns>` of conditional-install `<pattern>` items.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct PatternList {
    /// The conditional-install patterns.
    #[serde(rename = "pattern", default)]
    pub patterns: Vec<Pattern>,
}

/// `<pattern>`: `<dependencies>` + `<files>`. Every pattern whose dependencies hold
/// (against accumulated flags + installed files) contributes its files.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Pattern {
    /// The dependencies that must hold for these files to install.
    #[serde(rename = "dependencies", default)]
    pub dependencies: Option<CompositeDependency>,
    /// The files installed when the dependencies hold.
    #[serde(rename = "files", default)]
    pub files: Option<FileList>,
}

/// `<dependencies operator="And|Or">`: a composite dependency (recursive).
///
/// quick-xml's serde flattens the interleaved child elements into the typed `Vec`s
/// below; an empty `<dependencies>` evaluates per its operator (And ⇒ true, Or ⇒ false).
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct CompositeDependency {
    /// `operator` — `And` (default) or `Or`.
    #[serde(rename = "@operator", default)]
    pub operator: Operator,
    /// `<fileDependency>` arms.
    #[serde(rename = "fileDependency", default)]
    pub file_deps: Vec<FileDependency>,
    /// `<flagDependency>` arms.
    #[serde(rename = "flagDependency", default)]
    pub flag_deps: Vec<FlagDependency>,
    /// `<gameDependency>` arms (version match).
    #[serde(rename = "gameDependency", default)]
    pub game_deps: Vec<VersionDependency>,
    /// `<fommDependency>` arms (version match).
    #[serde(rename = "fommDependency", default)]
    pub fomm_deps: Vec<VersionDependency>,
    /// Nested `<dependencies>` arms (recursion).
    #[serde(rename = "dependencies", default)]
    pub nested: Vec<CompositeDependency>,
}

/// A single resolved dependency arm (the [`crate::condition`] evaluator's input shape).
///
/// This is the logical view the evaluator walks; [`CompositeDependency`] is the raw
/// deserialized shape. `condition::eval` iterates the typed `Vec`s of a
/// `CompositeDependency` directly, so this enum is a convenience for callers/tests that
/// want to reason about a single arm.
#[derive(Debug, Clone, PartialEq)]
pub enum Dependency {
    /// A flag must equal a value.
    Flag {
        /// Flag name.
        flag: String,
        /// Required value.
        value: String,
    },
    /// A file must be in a given state.
    File {
        /// File path.
        file: String,
        /// Required state.
        state: FileState,
    },
    /// A game/fomm version constraint.
    Version {
        /// Required minimum version string.
        version: String,
    },
    /// A nested composite dependency.
    Nested(Box<CompositeDependency>),
}

/// `<flagDependency flag="..." value="...">`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FlagDependency {
    /// The flag the dependency reads.
    #[serde(rename = "@flag", default)]
    pub flag: String,
    /// The value the flag must equal.
    #[serde(rename = "@value", default)]
    pub value: String,
}

/// `<fileDependency file="..." state="Missing|Inactive|Active">`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FileDependency {
    /// The file the dependency reads.
    #[serde(rename = "@file", default)]
    pub file: String,
    /// The required file state.
    #[serde(rename = "@state", default)]
    pub state: FileState,
}

/// The FOMOD file-dependency state enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum FileState {
    /// File is not present.
    Missing,
    /// File is present but inactive.
    Inactive,
    /// File is present and active.
    #[default]
    Active,
}

/// `<gameDependency version="...">` / `<fommDependency version="...">`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct VersionDependency {
    /// The required minimum version string.
    #[serde(rename = "@version", default)]
    pub version: String,
}

/// The `operator` attribute of a `<dependencies>` element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum Operator {
    /// All arms must hold.
    #[default]
    And,
    /// Any arm must hold.
    Or,
}

/// The `order` attribute on `<installSteps>`, `<optionalFileGroups>`, `<plugins>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum OrderKind {
    /// Sort ascending by name.
    #[default]
    Ascending,
    /// Sort descending by name.
    Descending,
    /// Preserve document order.
    Explicit,
}
