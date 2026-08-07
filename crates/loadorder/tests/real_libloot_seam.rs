//! libloot driven against REAL Skyrim SE plugins.
//!
//! `loot.rs` is the Linux seam onto libloot: it constructs a `Game` at a Proton-prefix
//! AppData path, asks libloot for the canonical order (which places the game master, the
//! hardcoded DLC list and CCC entries at their required fixed positions), splices the
//! user's controllable plugins into that, and round-trips the asterisk-format
//! `plugins.txt`.
//!
//! Every other test feeds it hand-written 24-byte TES4 headers. libloot reads real header
//! records, resolves real masters, and knows the real hardcoded early-loader list for
//! Skyrim SE — so a synthetic fixture cannot tell you whether the seam actually works.
//! This drives it over the genuine 80-plugin set.
//!
//! Skipped unless the sandbox exists (populate with `scripts/realtest-setup.sh`, which
//! only ever READS the real install).

use std::fs;
use std::path::PathBuf;

use loadorder::{
    appdata_folder_name, appdata_local_path, apply_load_order, enabled_names, esplugin_game_id,
    protected_plugins, read_plugins_txt, scan_plugin_views_for, view_to_plugin,
};
use nextwist_core::Plugin;

const SKYRIM_SE: u32 = 489830;

/// A minimal but genuinely esplugin-parseable TES4 header record.
fn tes4_header(master: bool) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(24);
    bytes.extend_from_slice(b"TES4");
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&(if master { 0x1u32 } else { 0 }).to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes
}

fn sandbox() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/realtest/game")
        .canonicalize()
        .ok()?;
    p.join("Data").is_dir().then_some(p)
}

/// Copy the sandbox Data tree into a per-run dir. libloot writes `plugins.txt` into the
/// prefix, not the game dir, but the game dir is opened read-write by the handle, so a
/// private copy keeps the shared sandbox unforgeable.
fn stage_copy(src: &std::path::Path, dst: &std::path::Path) {
    fs::create_dir_all(dst.join("Data")).unwrap();
    for entry in fs::read_dir(src.join("Data")).unwrap() {
        let p = entry.unwrap().path();
        if p.is_file() {
            fs::copy(&p, dst.join("Data").join(p.file_name().unwrap())).unwrap();
        }
    }
}

/// The full seam: open a real game handle, discover real plugins, apply an order, and
/// read back the asterisk-format `plugins.txt` libloot wrote.
#[test]
fn apply_load_order_round_trips_real_skyrim_plugins() {
    let Some(source) = sandbox() else {
        eprintln!("SKIP: no target/realtest/game sandbox (run scripts/realtest-setup.sh)");
        return;
    };
    let tmp = tempfile::TempDir::new().unwrap();
    let install = tmp.path().join("game");
    stage_copy(&source, &install);

    let folder = appdata_folder_name(SKYRIM_SE).expect("Skyrim SE is a supported game");
    let prefix = tmp.path().join("prefix");
    let appdata_local = appdata_local_path(&prefix, folder);
    fs::create_dir_all(&appdata_local).unwrap();

    // Discover the real plugin set the way the command layer does.
    let esplugin_id = esplugin_game_id(SKYRIM_SE).expect("Skyrim SE has a classifier");
    let mut views = scan_plugin_views_for(esplugin_id, &[], &install.join("Data")).unwrap();
    for v in &mut views {
        v.enabled = true;
    }
    let plugins: Vec<Plugin> = views.iter().cloned().map(view_to_plugin).collect();
    eprintln!("discovered {} real plugins", plugins.len());
    assert!(plugins.len() >= 50);

    // The protected set is libloot's own truth about implicitly-active plugins: everything
    // libloot reports ACTIVE that NexTwist did not itself enable. The realistic case is a
    // user who has enabled nothing yet — the game's hardcoded early-loaders must still come
    // back protected, so the UI can lock those rows. This also proves libloot opens a REAL
    // game directory at a REAL Proton-prefix AppData path.
    let nothing_enabled = std::collections::HashSet::new();
    let protected = protected_plugins(SKYRIM_SE, &install, &appdata_local, &nothing_enabled)
        .expect("protected query must succeed against a real install");
    eprintln!("libloot reports {} protected plugins", protected.len());
    assert!(
        protected
            .iter()
            .any(|p| p.eq_ignore_ascii_case("Skyrim.esm")),
        "the game master must be protected: {protected:?}"
    );

    // And the definition holds the other way: a plugin NexTwist enabled is never reported
    // protected, or the UI would lock a row the user is entitled to control.
    let all_enabled = enabled_names(&views);
    let none_protected = protected_plugins(SKYRIM_SE, &install, &appdata_local, &all_enabled)
        .expect("protected query must succeed");
    assert!(
        none_protected.is_empty(),
        "plugins we enabled ourselves must not be reported as protected: {none_protected:?}"
    );

    // Add a regular .esp so the file has something to carry: the real Skyrim set is all
    // masters and light plugins, and libloot never asterisk-writes those — their activation
    // is implicit. A user's mod .esp is the only thing that legitimately appears.
    let esp = install.join("Data/UserMod.esp");
    fs::write(&esp, tes4_header(false)).unwrap();
    let mut desired = plugins.clone();
    desired.push(Plugin {
        name: "UserMod.esp".into(),
        kind: nextwist_core::PluginKind::Esp,
        enabled: true,
        order: desired.len() as u32,
    });

    let written = apply_load_order(SKYRIM_SE, &install, &appdata_local, &desired)
        .expect("applying a real load order must succeed");
    eprintln!("wrote {}", written.display());
    assert!(written.is_file(), "plugins.txt must exist on disk");

    // Round-trip through the engine's own reader.
    let contents = read_plugins_txt(&appdata_local).expect("plugins.txt must be readable");
    let lines: Vec<&str> = contents.lines().filter(|l| !l.trim().is_empty()).collect();
    eprintln!("plugins.txt entries: {lines:?}");

    // The user's .esp is present and asterisk-marked as enabled.
    assert!(
        lines.iter().any(|l| l.eq_ignore_ascii_case("*UserMod.esp")),
        "the enabled user plugin must be asterisk-written: {lines:?}"
    );

    // libloot does NOT list the implicitly-active game master — masters are active without
    // a `*` line, and writing one would be wrong. A synthetic fixture cannot verify this,
    // because it depends on libloot's real hardcoded early-loader knowledge.
    assert!(
        !lines
            .iter()
            .any(|l| l.trim_start_matches('*').eq_ignore_ascii_case("Skyrim.esm")),
        "the implicitly-active game master must not be listed: {lines:?}"
    );

    // The user's .esp must sort AFTER every master-group plugin. libloot enforces
    // masters-first internally, and `reconcile_order` must keep its fixed early-loader
    // prefix intact rather than splicing a regular plugin in among them. A first sabotage
    // attempt (emptying the movable set) slipped past an earlier version of this test,
    // which is why the position is asserted explicitly rather than just the presence.
    let user_idx = lines
        .iter()
        .position(|l| l.eq_ignore_ascii_case("*UserMod.esp"))
        .expect("the user plugin must be listed");
    assert_eq!(
        user_idx,
        lines.len() - 1,
        "a regular .esp must sort after every master-group plugin: {lines:?}"
    );
    assert!(
        lines[..user_idx].iter().all(|l| {
            let n = l.trim_start_matches('*').to_lowercase();
            n.ends_with(".esm") || n.ends_with(".esl")
        }),
        "everything before the user plugin must be master-group: {lines:?}"
    );

    // Applying the same desired state twice is a no-op.
    let again = apply_load_order(SKYRIM_SE, &install, &appdata_local, &desired).unwrap();
    let contents2 = read_plugins_txt(&appdata_local).unwrap();
    assert_eq!(written, again);
    assert_eq!(
        contents, contents2,
        "re-applying an unchanged order must be a no-op"
    );
}

/// The user's chosen order among their OWN plugins must survive, spliced into libloot's
/// canonical early-loader prefix rather than replacing it.
///
/// This is the case where `reconcile_order` earns its keep, and it needs more than one
/// movable plugin to be observable: with a single `.esp` (or none) libloot's canonical
/// order passes through unchanged, so a sabotaged movable set still looks correct. Two
/// user plugins in a deliberate non-alphabetical order make the splice visible.
#[test]
fn the_users_own_plugin_order_survives_against_a_real_install() {
    let Some(source) = sandbox() else {
        eprintln!("SKIP: no target/realtest/game sandbox (run scripts/realtest-setup.sh)");
        return;
    };
    let tmp = tempfile::TempDir::new().unwrap();
    let install = tmp.path().join("game");
    stage_copy(&source, &install);

    let folder = appdata_folder_name(SKYRIM_SE).unwrap();
    let prefix = tmp.path().join("prefix");
    let appdata_local = appdata_local_path(&prefix, folder);
    fs::create_dir_all(&appdata_local).unwrap();

    let esplugin_id = esplugin_game_id(SKYRIM_SE).unwrap();
    let mut views = scan_plugin_views_for(esplugin_id, &[], &install.join("Data")).unwrap();
    for v in &mut views {
        v.enabled = true;
    }
    let mut desired: Vec<Plugin> = views.iter().cloned().map(view_to_plugin).collect();

    // Three user mods, requested in an order that is NOT alphabetical, so a lost or
    // re-sorted user order is visible rather than coincidentally right.
    for (i, name) in ["ZebraMod.esp", "AlphaMod.esp", "MiddleMod.esp"]
        .into_iter()
        .enumerate()
    {
        fs::write(install.join("Data").join(name), tes4_header(false)).unwrap();
        desired.push(Plugin {
            name: name.into(),
            kind: nextwist_core::PluginKind::Esp,
            enabled: true,
            order: (desired.len() + i) as u32,
        });
    }

    apply_load_order(SKYRIM_SE, &install, &appdata_local, &desired).unwrap();
    let contents = read_plugins_txt(&appdata_local).unwrap();
    let lines: Vec<&str> = contents.lines().filter(|l| !l.trim().is_empty()).collect();

    let pos = |n: &str| {
        lines
            .iter()
            .position(|l| l.eq_ignore_ascii_case(&format!("*{n}")))
            .unwrap_or_else(|| panic!("{n} must be listed: {lines:?}"))
    };
    let (z, a, m) = (
        pos("ZebraMod.esp"),
        pos("AlphaMod.esp"),
        pos("MiddleMod.esp"),
    );
    eprintln!("user plugin positions: Zebra={z} Alpha={a} Middle={m}");
    assert!(
        z < a && a < m,
        "the user's requested order (Zebra, Alpha, Middle) must be preserved: {lines:?}"
    );

    // And they still sit after every master-group plugin.
    assert!(
        lines[..z].iter().all(|l| {
            let n = l.trim_start_matches('*').to_lowercase();
            n.ends_with(".esm") || n.ends_with(".esl")
        }),
        "user plugins must follow the master group: {lines:?}"
    );
}
