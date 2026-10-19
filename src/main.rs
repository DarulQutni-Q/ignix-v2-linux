mod app;
mod device;
mod protocol;

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1120.0, 820.0])
        .with_min_inner_size([900.0, 640.0])
        .with_title("Ignix V2");
    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "ignix-v2",
        options,
        Box::new(|cc| {
            app::apply_style(&cc.egui_ctx);
            Ok(Box::new(app::IgnixApp::new()))
        }),
    )
}

/// PNG -> egui icon; app still launches without the asset.
fn load_icon() -> Option<egui::IconData> {
    const PNG: &[u8] = include_bytes!("../assets/icon.png");
    let img = image::load_from_memory(PNG).ok()?;
    let rgba = img.to_rgba8();
    Some(egui::IconData {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

use eframe::egui;

