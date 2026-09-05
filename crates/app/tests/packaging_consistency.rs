// SPDX-License-Identifier: AGPL-3.0-or-later
//! The version and app id live in several files. These tests make the build
//! enforce that they agree, so a release can't ship half-bumped.
use std::fs;
use std::path::PathBuf;

const APP_ID: &str = "io.github.genneth.Vernier";

fn packaging(name: &str) -> String {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "..", "packaging", name]
        .iter()
        .collect();
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn newest_metainfo_release_matches_cargo_version() {
    let xml = packaging(&format!("{APP_ID}.metainfo.xml"));
    let first_release = xml
        .split("<release ")
        .nth(1)
        .expect("metainfo has a <release>");
    let version = first_release
        .split("version=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("release has a version attribute");
    assert_eq!(
        version,
        env!("CARGO_PKG_VERSION"),
        "packaging/{APP_ID}.metainfo.xml's newest <release> must match Cargo.toml"
    );
}

#[test]
fn metainfo_desktop_and_manifest_use_the_app_id() {
    let xml = packaging(&format!("{APP_ID}.metainfo.xml"));
    assert!(xml.contains(&format!("<id>{APP_ID}</id>")));
    assert!(xml.contains(&format!(
        "<launchable type=\"desktop-id\">{APP_ID}.desktop</launchable>"
    )));
    let desktop = packaging(&format!("{APP_ID}.desktop"));
    assert!(desktop.contains(&format!("Icon={APP_ID}")));
    let manifest = packaging(&format!("{APP_ID}.yml"));
    assert!(manifest.contains(&format!("app-id: {APP_ID}")));
}
