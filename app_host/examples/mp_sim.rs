//! The player-cap sim (`jetpack-mp.md` §6 step 5): a `net.match` game for 2, 4, 6 and 8 players
//! over LAN, good wifi, 4G, and 4G with one bad link, through `app_host::net_sim`.
//!
//!     cargo run --release -p app_host --example mp_sim                       # mp_dots
//!     cargo run --release -p app_host --example mp_sim -- demo_apps/jetpack  # any match app
//!     cargo run --release -p app_host --example mp_sim -- DIR SECS
//!
//! The app must start its match from elements `script` (scripted input) and `start`, as
//! `mp_dots` does. Per row: rollbacks and stalled render frames per second, the mean frames a
//! rollback ran again, and the slowest peer's real ms per render frame (p50, p99, max).

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
    out
}

fn pct(v: &mut [f64], q: f64) -> f64 {
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * q) as usize]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let dir = args.first().map(PathBuf::from).unwrap_or_else(|| root.join("demo_apps/mp_dots"));
    let secs: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(20.0);
    let src = files(&dir);
    let id = "dots"; // the match id stats are read under; mp_dots' — overridden below if absent
    let profiles: [(&str, fn(usize) -> Link); 4] = [
        ("lan", |_| Link::LAN),
        ("wifi", |_| Link::WIFI),
        ("4g", |_| Link::FOUR_G),
        ("4g+1bad", |i| if i == 0 { Link::BAD } else { Link::FOUR_G }),
    ];
    println!("{} for {secs}s per row", dir.display());
    println!("{:>7} {:>8} {:>7} {:>9} {:>9} {:>8} {:>8} {:>8} {:>8} {:>7}", "players", "net", "frames", "rb/s", "rb len", "stall/s", "ms p50", "ms p99", "ms max", "desync");
    for n in [2usize, 4, 6, 8] {
        for (name, link) in profiles {
            let links = (0..n).map(link).collect();
            let mut sim = Sim::new_each(vec![src.clone(); n], links, 42);
            sim.click("script");
            sim.click("start");
            sim.run(secs, 1.0 / 60.0);
            let stats: Vec<_> = sim
                .apps
                .iter()
                .map(|a| {
                    let s = a.match_stats();
                    s.get(id).cloned().or_else(|| s.into_values().next()).unwrap_or_default()
                })
                .collect();
            let worst = stats.iter().max_by_key(|s| s.rolled_frames).cloned().unwrap_or_default();
            let mut ms: Vec<f64> = sim.frame_ms.iter().flatten().copied().collect();
            let frames = stats.iter().map(|s| s.frames).min().unwrap_or(0);
            let desync: u64 = stats.iter().map(|s| s.desyncs).sum();
            let rb_len = if worst.rollbacks == 0 { 0.0 } else { worst.rolled_frames as f64 / worst.rollbacks as f64 };
            let stalls = stats.iter().map(|s| s.stalls).max().unwrap_or(0);
            println!(
                "{n:>7} {name:>8} {frames:>7} {:>9.1} {rb_len:>9.1} {:>8.1} {:>8.3} {:>8.3} {:>8.3} {desync:>7}",
                worst.rollbacks as f64 / secs,
                stalls as f64 / secs,
                pct(&mut ms, 0.5),
                pct(&mut ms, 0.99),
                pct(&mut ms, 1.0),
            );
        }
    }
}
