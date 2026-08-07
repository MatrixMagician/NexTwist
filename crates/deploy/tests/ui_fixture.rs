//! Populate a throwaway app-data database so the deeper UI sections render with real
//! content. Not a test of behaviour — a fixture builder for looking at the app.
//!
//! Run with:
//!   cargo test -p nextwist-deploy --test ui_fixture -- --ignored --nocapture

use std::fs;
use std::path::{Path, PathBuf};

use deploy::{ModInput, deploy_winners, resolve};
use nextwist_core::{Game, ManagedMod};
use store::Store;

const APPID: u32 = 489830;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn copy_tree(src: &Path, dst: &Path) {
    fs::create_dir_all(dst.join("Data")).unwrap();
    for entry in fs::read_dir(src.join("Data")).unwrap() {
        let p = entry.unwrap().path();
        if p.is_file() {
            fs::copy(&p, dst.join("Data").join(p.file_name().unwrap())).unwrap();
        }
    }
}

#[test]
#[ignore = "fixture builder for manual UI inspection, not a behaviour test"]
fn build_ui_fixture() {
    let repo = repo();
    let sandbox = repo.join("target/realtest/game");
    if !sandbox.join("Data").is_dir() {
        eprintln!("SKIP: no target/realtest/game (run scripts/realtest-setup.sh)");
        return;
    }

    // A private copy of the real game, so the shared sandbox stays pristine.
    let install = repo.join("target/uitest/game");
    let _ = fs::remove_dir_all(&install);
    copy_tree(&sandbox, &install);

    let app = repo.join("target/uitest/xdg/com.nextwist.app");
    fs::create_dir_all(&app).unwrap();
    let staging = repo.join("target/uitest/staging/489830");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).unwrap();

    let game = Game {
        appid: APPID,
        name: "Skyrim Special Edition".into(),
        install_dir: install.clone(),
        prefix: repo.join("target/uitest/prefix"),
        staging_dir: staging.clone(),
    };

    let store = Store::open(&app.join("nextwist.db")).unwrap();
    store.add_managed_game(&game).unwrap();

    // Two mods that CONTEST a file, so the conflicts section has something to show.
    let mut inputs = Vec::new();
    for (name, rank, marker) in [
        ("Unofficial Patch", 1u32, &b"UP"[..]),
        ("Better Textures", 2, &b"BT"[..]),
    ] {
        let root = staging.join(name);
        fs::create_dir_all(root.join("Data/textures")).unwrap();
        fs::write(root.join("Data/textures/rock.dds"), marker).unwrap();
        fs::write(
            root.join(format!("Data/{}.esp", name.replace(' ', ""))),
            marker,
        )
        .unwrap();
        let id = store
            .add_mod(
                APPID,
                &ManagedMod {
                    id: 0,
                    name: name.into(),
                    staging_root: root.clone(),
                    enabled: true,
                    rank,
                },
            )
            .unwrap();
        inputs.push(ModInput {
            mod_id: id,
            staging_root: root,
            rank,
        });
    }

    let (winners, conflicts) = resolve(&inputs).unwrap();
    let report = deploy_winners(&store, &game, &winners).unwrap();

    // A second profile so the profiles section is non-trivial.
    let default = store.create_profile(APPID, "Default").unwrap();
    store.create_profile(APPID, "Testing").unwrap();
    store.set_active_profile(APPID, default).unwrap();

    eprintln!(
        "fixture ready: {} deployed, {} conflicts, install={}",
        report.deployed,
        conflicts.len(),
        install.display()
    );
}
