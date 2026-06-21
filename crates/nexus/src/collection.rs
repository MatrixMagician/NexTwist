//! Pure `collection.json` parser (COLL-01).
//!
//! Deserialises a NexusMods **Collection revision manifest** (Vortex's `ICollection`
//! shape, authored by `extensions/collections/src/types/ICollection.ts`) into a typed
//! [`Collection`] with NO I/O and NO Tauri — exactly the pure-parse discipline of
//! `crates/store/src/nexus.rs` and the FOMOD parser, so it is unit-testable headless.
//!
//! The manifest is **untrusted input** (a trust boundary, T-04-08): it lives inside the
//! attacker-authorable collection archive. Parsing it is allocation-bounded by `serde_json`
//! (T-04-11 accept); nothing here fetches a URL or touches disk — the resolve gate
//! ([`crate::resolve`]) decides what, if anything, is actionable, and off-Nexus source URLs
//! are NEVER auto-fetched.
//!
//! Naming follows the Collection Manifest Reference: `info` / `mods` / `mod_rules`; each
//! mod carries `source` (`type` + nexus identity), `choices` (the FOMOD replay), `phase`,
//! `file_overrides`, `patches`; each `modRule` carries `before`/`after`/`conflicts`/… over an
//! [`ModReference`]. Optional elements use `#[serde(default)]` so a sparse real-world
//! manifest (most fields are optional) parses cleanly.

use serde::{Deserialize, Serialize};

use crate::error::NexusError;

/// A parsed Collection revision manifest (`collection.json`).
///
/// `ICollection = { info, mods[], modRules[], collectionConfig? }`. We model the three
/// load-bearing members; `collectionConfig` is not needed for resolve/persist and is
/// ignored (serde drops unknown fields by default).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collection {
    /// Collection-level metadata (`ICollectionInfo`).
    pub info: CollectionInfo,
    /// The pinned mod list (`ICollectionMod[]`).
    #[serde(default)]
    pub mods: Vec<CollectionMod>,
    /// Ordering / conflict rules between mods (`ICollectionModRule[]`).
    #[serde(default, rename = "modRules")]
    pub mod_rules: Vec<CollectionModRule>,
}

/// Collection-level metadata (`ICollectionInfo`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionInfo {
    /// Collection author display name.
    #[serde(default)]
    pub author: String,
    /// Author profile URL (optional).
    #[serde(default, rename = "authorUrl")]
    pub author_url: Option<String>,
    /// Collection display name.
    pub name: String,
    /// Long description (optional).
    #[serde(default)]
    pub description: Option<String>,
    /// Free-text install instructions shown to the user (optional).
    #[serde(default, rename = "installInstructions")]
    pub install_instructions: Option<String>,
    /// The NexusMods game domain the Collection targets (e.g. `skyrimspecialedition`).
    #[serde(rename = "domainName")]
    pub domain_name: String,
    /// Supported game versions (optional).
    #[serde(default, rename = "gameVersions")]
    pub game_versions: Vec<String>,
}

/// One pinned mod inside the Collection (`ICollectionMod`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionMod {
    /// Mod display name.
    pub name: String,
    /// Pinned version string.
    #[serde(default)]
    pub version: String,
    /// Whether the mod is optional in the Collection.
    #[serde(default)]
    pub optional: bool,
    /// The game domain this mod belongs to (usually equals the Collection's).
    #[serde(default, rename = "domainName")]
    pub domain_name: Option<String>,
    /// Where the mod is acquired from (`ICollectionSourceInfo`).
    pub source: SourceInfo,
    /// The replayed FOMOD choices (`{type:"fomod", options:[…]}`), when the mod pins one.
    #[serde(default)]
    pub choices: Option<Choices>,
    /// Binary patches keyed by file path (optional).
    #[serde(default)]
    pub patches: Option<serde_json::Map<String, serde_json::Value>>,
    /// Manual instructions shown to the user (optional).
    #[serde(default)]
    pub instructions: Option<String>,
    /// Install ordering phase (0-based). Absent ⇒ phase 0.
    #[serde(default)]
    pub phase: u32,
    /// File paths this mod force-wins in conflict resolution (optional).
    #[serde(default, rename = "fileOverrides")]
    pub file_overrides: Vec<String>,
}

/// The acquisition source for a Collection mod (`ICollectionSourceInfo`).
///
/// `type` discriminates how (and whether) NexTwist may obtain the file. Only `nexus` and
/// `bundle` are actionable; `direct`/`browse`/`manual` are off-Nexus and surfaced as
/// manual steps — they are NEVER auto-fetched (locked decision; T-04-08 SSRF mitigation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceInfo {
    /// The source kind.
    #[serde(rename = "type")]
    pub kind: SourceType,
    /// NexusMods mod id (present for `nexus` sources).
    #[serde(default, rename = "modId")]
    pub mod_id: Option<u64>,
    /// NexusMods file id (present for `nexus` sources).
    #[serde(default, rename = "fileId")]
    pub file_id: Option<u64>,
    /// The pinned file's md5 (optional).
    #[serde(default)]
    pub md5: Option<String>,
    /// The pinned file's size in bytes (optional).
    #[serde(default, rename = "fileSize")]
    pub file_size: Option<u64>,
    /// Off-Nexus URL (present for `direct`/`browse`). Carried but NEVER auto-fetched.
    #[serde(default)]
    pub url: Option<String>,
    /// Source-specific manual instructions (optional).
    #[serde(default)]
    pub instructions: Option<String>,
    /// Update policy (`exact`/`latest`/`prefer`) (optional).
    #[serde(default, rename = "updatePolicy")]
    pub update_policy: Option<String>,
    /// Logical filename matcher (optional).
    #[serde(default, rename = "logicalFilename")]
    pub logical_filename: Option<String>,
    /// File expression matcher (optional).
    #[serde(default, rename = "fileExpression")]
    pub file_expression: Option<String>,
    /// Opaque source tag (optional).
    #[serde(default)]
    pub tag: Option<String>,
}

/// The kind of a Collection mod's source (`ICollectionSourceInfo.type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    /// A pinned NexusMods file (`modId`+`fileId`) — downloadable via the Phase-3 client.
    Nexus,
    /// A file bundled inside the collection archive itself — no download.
    Bundle,
    /// An externally hosted direct download — off-Nexus, NEVER auto-fetched.
    Direct,
    /// A page the user must visit to obtain the file — off-Nexus, manual.
    Browse,
    /// A file the user must supply manually — off-Nexus, manual.
    Manual,
}

impl SourceType {
    /// Whether this source is off-Nexus and must be surfaced as a manual step rather than
    /// fetched (the SSRF-safe classification, T-04-08).
    pub fn is_off_nexus(self) -> bool {
        matches!(self, SourceType::Direct | SourceType::Browse | SourceType::Manual)
    }
}

/// The FOMOD-replay encoding (`IChoices`): an ordered list of steps by name, each with
/// groups by name, each with the chosen options by name + index. Driven directly by
/// `crates/fomod::resolve` in the headless Collection-install path (Plan 04 / COLL-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choices {
    /// The choice encoding type — always `"fomod"` for a scripted-installer replay.
    #[serde(rename = "type")]
    pub kind: String,
    /// The chosen steps.
    #[serde(default)]
    pub options: Vec<ChoiceStep>,
}

/// One FOMOD install step in the replay (`IChoices.options[]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoiceStep {
    /// The install-step name (matched against the parsed `ModuleConfig.xml`).
    pub name: String,
    /// The groups chosen within this step.
    #[serde(default)]
    pub groups: Vec<ChoiceGroup>,
}

/// One FOMOD group in the replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoiceGroup {
    /// The group name.
    pub name: String,
    /// The options chosen within this group.
    #[serde(default)]
    pub choices: Vec<ChoiceOption>,
}

/// One chosen FOMOD option in the replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoiceOption {
    /// The option name.
    pub name: String,
    /// The option index within its group.
    #[serde(default)]
    pub idx: u32,
}

/// A rule ordering or relating two Collection mods (`ICollectionModRule`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionModRule {
    /// The rule kind (`before`/`after`/`requires`/`conflicts`/`recommends`/`provides`).
    #[serde(rename = "type")]
    pub kind: ModRuleType,
    /// The mod the rule applies FROM (`IModReference`).
    pub source: ModReference,
    /// The mod the rule applies TO (`IModReference`).
    pub reference: ModReference,
}

/// The kind of a `modRule` (`ICollectionModRule.type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModRuleType {
    /// `source` loads before `reference`.
    Before,
    /// `source` loads after `reference`.
    After,
    /// `source` requires `reference`.
    Requires,
    /// `source` conflicts with `reference`.
    Conflicts,
    /// `source` recommends `reference`.
    Recommends,
    /// `source` provides `reference`.
    Provides,
}

/// A matching predicate identifying a mod in a `modRule` (`IModReference`).
///
/// A reference matches a resolved mod by `tag`, `md5`, `logicalFileName` (+ `versionMatch`),
/// `fileExpression`, or `repo` (modId/fileId). All fields are optional; an empty reference
/// (or one matching no resolved mod) is skipped, not fatal (Pitfall 4 / T-04-09).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ModReference {
    /// Opaque tag matcher.
    #[serde(default)]
    pub tag: Option<String>,
    /// md5 hint matcher.
    #[serde(default, rename = "md5Hint")]
    pub md5_hint: Option<String>,
    /// Exact file md5 matcher.
    #[serde(default, rename = "fileMD5")]
    pub file_md5: Option<String>,
    /// id hint matcher.
    #[serde(default, rename = "idHint")]
    pub id_hint: Option<String>,
    /// Logical filename matcher.
    #[serde(default, rename = "logicalFileName")]
    pub logical_file_name: Option<String>,
    /// Version match expression (paired with `logicalFileName`).
    #[serde(default, rename = "versionMatch")]
    pub version_match: Option<String>,
    /// File expression matcher.
    #[serde(default, rename = "fileExpression")]
    pub file_expression: Option<String>,
}

impl Collection {
    /// Parse a `collection.json` manifest string into a typed [`Collection`].
    ///
    /// Pure (no I/O). A malformed or schema-invalid manifest flattens the serde error into a
    /// [`NexusError::Http`] string at the crate boundary (the crate's String-flattening
    /// convention) — never a panic on this untrusted input (T-04-08 / T-04-11).
    pub fn parse(json: &str) -> Result<Collection, NexusError> {
        serde_json::from_str(json)
            .map_err(|e| NexusError::Http(format!("malformed collection.json: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/collection.json");

    #[test]
    fn parses_real_fixture_with_all_fields() {
        let c = Collection::parse(FIXTURE).expect("fixture must parse");

        // info
        assert_eq!(c.info.name, "Skyrim Essentials");
        assert_eq!(c.info.domain_name, "skyrimspecialedition");
        assert_eq!(c.info.game_versions, vec!["1.6.1170"]);
        assert_eq!(c.info.author, "ModderOne");

        // mods: 7 total
        assert_eq!(c.mods.len(), 7);

        // SkyUI: nexus source identity + FOMOD choices + fileOverrides + phase
        let skyui = &c.mods[0];
        assert_eq!(skyui.name, "SkyUI");
        assert_eq!(skyui.source.kind, SourceType::Nexus);
        assert_eq!(skyui.source.mod_id, Some(12604));
        assert_eq!(skyui.source.file_id, Some(120063));
        assert_eq!(
            skyui.source.md5.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(skyui.source.file_size, Some(1048576));
        assert_eq!(skyui.phase, 0);
        assert_eq!(skyui.file_overrides, vec!["interface/skyui.swf"]);

        // The IChoices FOMOD replay parses fully (step → group → option by name+idx).
        let choices = skyui.choices.as_ref().expect("SkyUI pins FOMOD choices");
        assert_eq!(choices.kind, "fomod");
        assert_eq!(choices.options.len(), 1);
        assert_eq!(choices.options[0].name, "Main Installation");
        assert_eq!(choices.options[0].groups[0].name, "UI Variant");
        assert_eq!(choices.options[0].groups[0].choices[0].name, "Full UI");
        assert_eq!(choices.options[0].groups[0].choices[0].idx, 0);

        // bundle source
        let bundle = c.mods.iter().find(|m| m.name == "Collection Config Patch").unwrap();
        assert_eq!(bundle.source.kind, SourceType::Bundle);
        assert!(bundle.patches.is_some());
        assert_eq!(bundle.phase, 2);

        // off-Nexus direct source carries a URL + instructions but is classified off-Nexus
        let skse = c.mods.iter().find(|m| m.name == "SKSE64").unwrap();
        assert_eq!(skse.source.kind, SourceType::Direct);
        assert!(skse.source.kind.is_off_nexus());
        assert!(skse.source.url.is_some());
        assert!(skse.instructions.is_some());

        // browse source is off-Nexus too
        let browse = c.mods.iter().find(|m| m.name == "Browse-Only Dependency").unwrap();
        assert_eq!(browse.source.kind, SourceType::Browse);
        assert!(browse.source.kind.is_off_nexus());

        // modRules: before/after/conflicts + a stale phantom reference
        assert_eq!(c.mod_rules.len(), 4);
        assert_eq!(c.mod_rules[0].kind, ModRuleType::After);
        assert_eq!(c.mod_rules[1].kind, ModRuleType::Before);
        assert_eq!(c.mod_rules[2].kind, ModRuleType::Conflicts);
        assert_eq!(
            c.mod_rules[0].source.tag.as_deref(),
            Some("skyui-tag")
        );
    }

    #[test]
    fn malformed_manifest_errors_not_panics() {
        let err = Collection::parse("{ not json").expect_err("malformed must error");
        assert!(matches!(err, NexusError::Http(_)));
        // A manifest missing the required `info.name` also errors cleanly.
        let err = Collection::parse(r#"{"info":{"domainName":"x"},"mods":[]}"#)
            .expect_err("missing required field must error");
        assert!(matches!(err, NexusError::Http(_)));
    }

    #[test]
    fn sparse_manifest_parses_with_defaults() {
        // The minimal valid shape: info.name + info.domainName, no mods, no rules.
        let c = Collection::parse(
            r#"{"info":{"name":"Tiny","domainName":"skyrimspecialedition"}}"#,
        )
        .expect("a sparse manifest must parse");
        assert_eq!(c.info.name, "Tiny");
        assert!(c.mods.is_empty());
        assert!(c.mod_rules.is_empty());
        assert_eq!(c.info.game_versions.len(), 0);
    }
}
