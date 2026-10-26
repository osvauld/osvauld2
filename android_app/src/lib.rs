//! Android APK entry point for the shared shell. Owns packaging, the activity handoff and
//! selection of app-private storage; rendering, accounts and screens remain shared.
#![cfg(target_os = "android")]

use runtime::AndroidApp;

#[unsafe(no_mangle)]
fn android_main(activity: AndroidApp) {
    let root = activity.internal_data_path().expect("Android private data directory");
    #[cfg(debug_assertions)]
    shell2::configure_android_debug(root.clone());
    let vault = vault::Vault::open(root.join("vault")).expect("open Android vault");
    runtime::run_android_with(activity, |proxy| shell2::Shell::new(proxy, vault));
}
