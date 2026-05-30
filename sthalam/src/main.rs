mod app;
mod components;
mod screens;
mod theme;
mod workspace;

use std::path::PathBuf;

use app::Sthalam;
use eframe::egui;

fn main() -> eframe::Result {
    // The driver resolves the data dir (env override, else the OS default); vault itself
    // never parses arguments.
    let data_dir = std::env::var_os("OSVAULD_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(vault::default_dir);

    let vault = vault::Vault::open(data_dir).expect("failed to open the osvauld data directory");

    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("osvauld")
            .with_inner_size([1040.0, 720.0])
            .with_min_inner_size([540.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "sthalam",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(Sthalam::new(vault)))
        }),
    )
}
