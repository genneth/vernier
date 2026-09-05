// SPDX-License-Identifier: AGPL-3.0-or-later
mod ui;

use adw::prelude::*;
use gtk4::gio;
use libadwaita as adw;
use tracing_subscriber::EnvFilter;

fn main() -> glib::ExitCode {
    // Logging: default INFO; override per-target via RUST_LOG, e.g.
    //   RUST_LOG=vernier=debug   or   RUST_LOG=vernier::ui::canvas=trace
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .init();
    glib::set_application_name("Vernier");
    tracing::info!("Vernier {} starting", env!("CARGO_PKG_VERSION"));

    // HANDLES_OPEN so the app can be launched with a PDF ("Open with…").
    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    app.connect_activate(|app| ui::build_window(app, None));
    app.connect_open(|app, files, _hint| {
        let path = files
            .first()
            .and_then(|f| f.path())
            .map(|p| p.to_string_lossy().into_owned());
        ui::build_window(app, path);
    });
    app.run()
}
