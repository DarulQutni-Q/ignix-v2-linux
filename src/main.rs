mod app;
mod device;
mod protocol;
#[cfg(test)]
mod tests;

use app::IgnixApp;

fn main() -> eframe::Result<()> {
    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([1024.0, 640.0])
            .with_title("IGNIX V2 — Linux Configurator"),
        ..Default::default()
    };
    eframe::run_native(
        "ignix-v2",
        opts,
        Box::new(|cc| Ok(Box::new(IgnixApp::new(cc)))),
    )
}
