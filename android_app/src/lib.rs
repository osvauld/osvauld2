//! Android APK entry point for the shared shell. Owns packaging, the activity handoff and
//! selection of app-private storage; rendering, accounts and screens remain shared.
#![cfg(target_os = "android")]

use runtime::AndroidApp;

#[cfg(debug_assertions)]
#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(
        prio: i32,
        tag: *const std::ffi::c_char,
        text: *const std::ffi::c_char,
    ) -> i32;
}

/// Debug builds: what the shell prints (`net:`, `kunki:`, a panic) reaches
/// `adb logcat -s osvauld`. An APK's own stdout and stderr otherwise go nowhere.
#[cfg(debug_assertions)]
fn stdio_to_logcat() {
    use std::io::{BufRead, BufReader};
    use std::os::fd::FromRawFd;
    let mut fds = [0i32; 2];
    // SAFETY: a fresh pipe; its write end is put over fds 1 and 2, its read end is owned by
    // the `File` below and read by one thread for the life of the process.
    let reader = unsafe {
        if libc::pipe(fds.as_mut_ptr()) != 0 {
            return;
        }
        libc::dup2(fds[1], 1);
        libc::dup2(fds[1], 2);
        std::fs::File::from_raw_fd(fds[0])
    };
    std::thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            if let Ok(line) = std::ffi::CString::new(line) {
                // SAFETY: both strings are NUL-terminated and outlive the call.
                unsafe { __android_log_write(4, c"osvauld".as_ptr(), line.as_ptr()) };
            }
        }
    });
}

/// Debug builds: the net stack's own tracing (iroh, its relay client, netwatch, the DNS
/// resolver) at debug level, everything else at warn, through the same pipe as stderr.
#[cfg(debug_assertions)]
fn trace_to_logcat() {
    use tracing_subscriber::filter::{LevelFilter, Targets};
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    let targets = Targets::new()
        .with_default(LevelFilter::WARN)
        .with_targets([
            ("iroh", LevelFilter::DEBUG),
            ("iroh_relay", LevelFilter::DEBUG),
            ("netwatch", LevelFilter::DEBUG),
            ("n0_dns_resolver", LevelFilter::DEBUG),
            ("net", LevelFilter::DEBUG),
        ]);
    let fmt = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .without_time();
    let _ = tracing_subscriber::registry().with(targets).with(fmt).try_init();
}

#[unsafe(no_mangle)]
fn android_main(activity: AndroidApp) {
    #[cfg(debug_assertions)]
    {
        stdio_to_logcat();
        trace_to_logcat();
    }
    let root = activity.internal_data_path().expect("Android private data directory");
    #[cfg(debug_assertions)]
    shell2::configure_android_debug(root.clone());
    let vault = vault::Vault::open(root.join("vault")).expect("open Android vault");
    runtime::run_android_with(activity, |proxy| shell2::Shell::new(proxy, vault));
}
