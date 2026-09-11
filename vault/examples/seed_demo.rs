//! Seeds a throwaway demo store: one account, one workspace, the `tally` app ready to open.
//!
//!     cargo run -p vault --example seed_demo -- /tmp/osvauld-demo
//!
//! Then launch the shell against it: `OSVAULD_DATA_DIR=/tmp/osvauld-demo cargo run -p shell2`.
//! The passphrase is `demo` — this store exists to be thrown away.

use std::path::PathBuf;

use loro::{LoroDoc, LoroText};
use vault::{ItemKind, Vault};

fn main() {
    let dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: seed_demo <data-dir>");
    let mut vault = Vault::open(dir).expect("open the demo data dir");

    if vault.is_empty() {
        let (did, words) = vault
            .signup("Demo", "demo")
            .expect("create the demo account");
        println!("account: {did}");
        println!("mnemonic (write it down if you care about this store):");
        for (i, w) in words.words().enumerate() {
            print!("{} ", w);
            if i % 6 == 5 {
                println!();
            }
        }
        println!();
    } else {
        let did = current_did(&vault);
        vault.login(&did, "demo").expect("unlock");
    }

    // Reuse the workspace if the script ran before, so re-seeding is idempotent.
    let ws = vault
        .workspaces()
        .expect("list workspaces")
        .into_iter()
        .find(|w| w.name == "Playground")
        .map(|w| w.id)
        .unwrap_or_else(|| vault.create_workspace("Playground").expect("workspace").id);

    // The app's source snapshot: the same shape shell2's upload produces — a `files` map
    // of LoroText, one entry per file in the uploaded folder.
    upsert_app(
        &mut vault,
        &ws,
        "tally",
        &[
            ("main.lua", include_str!("../../demo_apps/tally/main.lua")),
            (
                "manifest.osv",
                include_str!("../../demo_apps/tally/manifest.osv"),
            ),
        ],
    );
    upsert_app(
        &mut vault,
        &ws,
        "scratch",
        &[
            ("main.lua", include_str!("../../demo_apps/scratch/main.lua")),
            (
                "manifest.osv",
                include_str!("../../demo_apps/scratch/manifest.osv"),
            ),
        ],
    );
}

/// Replace any earlier copy rather than stacking duplicates, so re-seeding is idempotent.
fn upsert_app(vault: &mut Vault, ws: &str, name: &str, files: &[(&str, &str)]) {
    let src = app_snapshot(files);
    if let Ok(items) = vault.items(ws) {
        for it in items.iter().filter(|i| i.name == name) {
            vault.put_src(ws, &it.id, &src).expect("update the source");
            println!("updated item {name} ({})", it.id);
            return;
        }
    }
    let item = vault
        .create_item(ws, name, ItemKind::App)
        .expect("create the item");
    vault.put_src(ws, &item.id, &src).expect("write the source");
    println!("created item {name} ({}) in Playground ({ws})", item.id);
}

fn current_did(vault: &Vault) -> String {
    vault
        .accounts()
        .expect("list accounts")
        .into_iter()
        .next()
        .expect("one account in the demo store")
        .did
}

fn app_snapshot(files: &[(&str, &str)]) -> Vec<u8> {
    let doc = LoroDoc::new();
    let map = doc.get_map("files");
    for (path, contents) in files {
        let text = map
            .insert_container(path, LoroText::new())
            .expect("insert the file");
        text.insert(0, contents).expect("write the file");
    }
    doc.export(loro::ExportMode::Snapshot).expect("snapshot")
}
