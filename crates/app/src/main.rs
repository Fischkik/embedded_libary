//! Desktop entry point: opens the window and hands every frame to the shell.

use embedded_lab::{Saved, Shell};

struct App {
    shell: Shell,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.shell.ui(ui);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.shell.saved);
    }
}

/// Under WSL the default OpenGL route (Mesa on top of Direct3D) drops the connection to
/// the display at start-up, while Vulkan works. Pick Vulkan there unless the user has
/// chosen a backend themselves with the `WGPU_BACKEND` environment variable.
#[cfg(not(target_arch = "wasm32"))]
fn prefer_working_graphics_backend() {
    if std::env::var_os("WGPU_BACKEND").is_none() && std::env::var_os("WSL_DISTRO_NAME").is_some() {
        // SAFETY: called as the first thing in `main`, before any other thread exists.
        unsafe { std::env::set_var("WGPU_BACKEND", "vulkan") };
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    prefer_working_graphics_backend();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Embedded Lab")
            .with_inner_size([1500.0, 950.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Embedded Lab",
        options,
        Box::new(|cc| {
            let saved: Saved = cc.storage.and_then(|s| eframe::get_value(s, eframe::APP_KEY)).unwrap_or_default();
            Ok(Box::new(App { shell: Shell::new(demos::registry(), saved) }))
        }),
    )
}

// The browser build is not wired up yet. Everything below `main.rs` (the shell, the
// demos, the simulation core) avoids threads, clocks and file access so that adding
// a web entry point here is all it takes.
#[cfg(target_arch = "wasm32")]
fn main() {}
