// SPDX-License-Identifier: AGPL-3.0-or-later
//! Vernier's headless core: PDF geometry extraction, snapping, scale and the
//! measuring tools. Unit-testable without GTK; the GTK shell in `crates/app`
//! is a thin projection of [`app::AppState`].
pub mod app;
pub mod geometry;
pub mod pdf;
pub mod scale;
pub mod snap;
pub mod tools;
pub mod view;
