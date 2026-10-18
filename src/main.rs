mod app;
mod device;
mod protocol;
#[cfg(test)]
mod tests;

use app::IgnixApp;

fn main() -> eframe::Result<()> {
    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("Ignix V2 Configurator"),
        ..Default::default()
    };
    eframe::run_native(
        "ignix-v2",
        opts,
        Box::new(|cc| Ok(Box::new(IgnixApp::new(cc)))),
    )
}
