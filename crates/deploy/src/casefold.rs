//! Case-sensitivity normalization against the per-game canonical `Data/` casing
//! casing.
//!
//! Wine/Proton does NOT abstract the filesystem: a Windows `open("Data\\Textures\\x")`
//! becomes a case-sensitive Linux `open()`. Mods are authored on case-insensitive NTFS,
//! so a mod may carry `TEXTURES/Foo.DDS` even though the game's real on-disk directory
//! is `Textures/`. Deployed verbatim onto a case-sensitive Linux tree, the game's
//! `open()` would miss the file and the mod would silently do nothing.
//!
//! This module rewrites every DIRECTORY component of an incoming mod relpath to the
//! game's REAL casing using the [`CasingMap`] produced by
//! `steam::canonical_data_casing`. Leaf filenames are preserved verbatim
//! (the casing map records directories only); a mod-introduced directory that the game
//! does not have keeps the mod's own casing (there is no canonical answer to defer to).
//! Normalization ALWAYS runs regardless of the best-effort `Casefold` probe so the
//! result is portable across filesystems.
//!
//! ## Known limitation: a wrong-case LEAF replacing a vanilla file
//!
//! Because leaves are verbatim, a mod shipping `Data/hearthfires.esm` against a game whose
//! real file is `Data/HearthFires.esm` deploys as a SECOND file on a case-sensitive
//! filesystem rather than replacing the original. Verified against a real Skyrim SE
//! install: the vanilla file is untouched, no backup is taken (correctly — nothing was
//! overwritten), and purge removes only the file we added, so the round trip stays
//! byte-for-byte pristine. The reversibility guarantee therefore holds; what the user gets
//! is a mod that silently does not take effect, which the `NotCasefolded` warning in the
//! deploy report is there to explain.
//!
//! Fixing it would mean extending the casing map to leaf filenames, which is a bigger and
//! riskier change than it looks: two vanilla files differing only in case are legal on
//! Linux, so a leaf map needs a collision policy, and picking the wrong winner would
//! overwrite a real game file. Left as-is deliberately.

use std::path::{Component, Path, PathBuf};

use steam::CasingMap;

/// Rewrite the directory components of `target_rel` to the game's canonical `Data/`
/// casing, preserving the leaf filename and any mod-introduced (game-absent) directory.
///
/// `target_rel` may be `Data/`-rooted (e.g. `DATA/Textures/x.dds`) or already
/// `Data/`-relative (e.g. `Textures/x.dds`). A leading `Data` segment (matched
/// case-insensitively) is rewritten to the map's real `data_dir_name`; the remainder is
/// mapped relative to `Data/`. Components are looked up by their accumulated lowercase
/// `/`-joined path against [`CasingMap::canonical_dir`]; a hit substitutes the canonical
/// casing of that component, a miss preserves the incoming casing.
///
/// Only `Normal` components are mapped; `.`/`..`/root components are passed through
/// unchanged (real deploy paths are validated `..`-free upstream, but we never panic).
pub fn normalize_to_canonical(target_rel: &Path, casing: &CasingMap) -> PathBuf {
    // Collect the Normal path components as owned strings; pass through anything else.
    let parts: Vec<String> = target_rel
        .components()
        .map(|c| match c {
            Component::Normal(os) => os.to_string_lossy().into_owned(),
            other => other.as_os_str().to_string_lossy().into_owned(),
        })
        .collect();

    if parts.is_empty() {
        return PathBuf::new();
    }

    let mut out = PathBuf::new();
    let mut rest_start = 0usize;

    // 1. A leading `Data` segment (case-insensitive) is rewritten to the canonical
    //    on-disk data dir name; the lowercase-relative lookup keys are taken relative
    //    to Data/ (the casing map is rooted at Data/).
    if parts[0].eq_ignore_ascii_case("data") {
        out.push(&casing.data_dir_name);
        rest_start = 1;
    }

    // 2. Walk the remaining components. Every component EXCEPT the last is a directory:
    //    look up its accumulated lowercase rel-path under Data/ and substitute the
    //    canonical casing of the final segment when the game has that directory.
    let rest = &parts[rest_start..];
    let last_idx = rest.len().saturating_sub(1);
    let mut lower_rel = String::new();

    for (i, comp) in rest.iter().enumerate() {
        if i == last_idx {
            // Leaf component (filename, or the only component) — preserve verbatim.
            out.push(comp);
            break;
        }
        // Directory component: extend the lowercase rel-path and look it up.
        if !lower_rel.is_empty() {
            lower_rel.push('/');
        }
        lower_rel.push_str(&comp.to_lowercase());

        match casing.canonical_dir(&lower_rel) {
            Some(canonical_rel) => {
                // canonical_rel is the FULL `/`-joined canonical path to this dir; the
                // last segment is this component's real casing.
                let canonical_leaf = canonical_rel.rsplit('/').next().unwrap_or(canonical_rel);
                out.push(canonical_leaf);
            }
            None => {
                // A mod-introduced directory the game lacks — keep the mod's casing.
                out.push(comp);
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn map_with(data_dir_name: &str, dirs: &[(&str, &str)]) -> CasingMap {
        let mut m = BTreeMap::new();
        for (k, v) in dirs {
            m.insert((*k).to_string(), (*v).to_string());
        }
        CasingMap {
            data_dir_name: data_dir_name.to_string(),
            dirs: m,
        }
    }

    #[test]
    fn single_dir_component_normalized() {
        let casing = map_with("Data", &[("textures", "Textures")]);
        assert_eq!(
            normalize_to_canonical(Path::new("TEXTURES/x.dds"), &casing),
            PathBuf::from("Textures/x.dds"),
        );
    }

    #[test]
    fn nested_dirs_use_per_segment_canonical_leaf() {
        let casing = map_with(
            "Data",
            &[
                ("textures", "Textures"),
                ("textures/actors", "Textures/Actors"),
            ],
        );
        assert_eq!(
            normalize_to_canonical(Path::new("TEXTURES/ACTORS/z.dds"), &casing),
            PathBuf::from("Textures/Actors/z.dds"),
        );
    }

    #[test]
    fn leaf_only_path_is_unchanged() {
        let casing = map_with("Data", &[]);
        assert_eq!(
            normalize_to_canonical(Path::new("Skyrim.esm"), &casing),
            PathBuf::from("Skyrim.esm"),
        );
    }

    #[test]
    fn game_absent_dir_keeps_mod_casing() {
        let casing = map_with("Data", &[("textures", "Textures")]);
        assert_eq!(
            normalize_to_canonical(Path::new("SOUND/boom.wav"), &casing),
            PathBuf::from("SOUND/boom.wav"),
        );
    }

    #[test]
    fn leading_data_segment_rewritten_to_canonical_name() {
        let casing = map_with("Data", &[("textures", "Textures")]);
        assert_eq!(
            normalize_to_canonical(Path::new("data/TEXTURES/x.dds"), &casing),
            PathBuf::from("Data/Textures/x.dds"),
        );
    }
}
