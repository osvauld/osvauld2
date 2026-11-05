//! Cross-device check (`jetpack-mp.md` step 11, M12): run a match app alone, headless, for a fixed
//! time on a fixed clock, and print its checkpoint hashes. Two devices that print the same lines
//! simulate the same game bit for bit.
//!
//!     cargo run --release -p app_host --example mp_hashes -- demo_apps/jetpack 60 match:start
//!     cargo run --release -p app_host --example mp_hashes -- demo_apps/mp_hockey 60 script start
//!
//! After the app dir and seconds come the element ids to click, in order, before the clock runs.
//! Jetpack alone is its bots against an idle pilot: rich enough — trig, rays, splash, the RNG.

use std::path::{Path, PathBuf};

use app_host::net_sim::{Link, Sim};

fn files(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).expect("read app dir") {
            let p = e.expect("entry").path();
            if p.is_dir() {
                stack.push(p);
            } else if matches!(p.extension().and_then(|x| x.to_str()), Some("lua" | "osv")) {
                let rel = p.strip_prefix(dir).unwrap().to_string_lossy().replace('\\', "/");
                out.push((rel, std::fs::read_to_string(&p).expect("read file")));
            }
        }
    }
    out.sort();
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = PathBuf::from(args.first().expect("an app dir"));
    let secs: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(60.0);
    let mut sim = Sim::new_each(vec![files(&dir)], vec![Link::LAN], 1);
    for id in args.iter().skip(2) {
        sim.click(id);
    }
    sim.run(secs, 1.0 / 60.0);
    let app = &sim.apps[0];
    for line in app.console(20) {
        eprintln!("console: {line}");
    }
    let stats = app.match_stats();
    let Some((id, _)) = stats.iter().next() else {
        eprintln!("no match ran");
        std::process::exit(1);
    };
    println!("{} {} {}", std::env::consts::ARCH, dir.display(), id);
    for (frame, hash) in app.match_checks(id) {
        println!("{frame} {hash}");
    }
}
