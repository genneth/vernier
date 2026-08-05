// SPDX-License-Identifier: AGPL-3.0-or-later
use vernier_core::pdf::{mupdf_backend::MupdfBackend, PdfBackend};

/// Synthetic two-page vector "floor plan", committed to the repo.
/// Regenerate with `tests/fixtures/make_fixture.py`.
fn fixture_path() -> String {
    format!("{}/tests/fixtures/plan.pdf", env!("CARGO_MANIFEST_DIR"))
}

fn backend() -> MupdfBackend {
    MupdfBackend::open(&fixture_path()).expect("open fixture")
}

#[test]
fn page_count_is_two() {
    assert_eq!(backend().page_count(), 2);
}

#[test]
fn extracts_substantial_geometry_from_page0() {
    let polylines = backend().extract_geometry(0).unwrap();
    let verts: usize = polylines.iter().map(|p| p.vertices().len()).sum();
    // The generator strokes ~100 paths on page 0 (walls, stairs, hatch, grid
    // ticks, door-swing beziers). Bounds are loose so cosmetic tweaks to the
    // fixture don't break the test.
    assert!(polylines.len() > 60, "got {} polylines", polylines.len());
    assert!(verts > 200, "got {verts} vertices");
}

#[test]
fn renders_page0_to_rgba() {
    let b = backend();
    let (pw, ph) = b.page_size_pts(0);
    let img = b.render_region(0, 2.0, (0.0, 0.0, pw, ph)).unwrap();
    assert_eq!(img.bytes.len(), (img.width * img.height * 4) as usize);
    assert!(img.width > 1000 && img.height > 1000);
    assert_eq!(img.scale, 2.0);
}

#[test]
fn clipped_region_is_smaller_and_offset() {
    let b = backend();
    let (pw, ph) = b.page_size_pts(0);
    let full = b.render_region(0, 2.0, (0.0, 0.0, pw, ph)).unwrap();
    // Render just the top-left quarter.
    let quarter = b
        .render_region(0, 2.0, (0.0, 0.0, pw / 2.0, ph / 2.0))
        .unwrap();
    assert!(quarter.width < full.width && quarter.height < full.height);
    assert_eq!(
        quarter.bytes.len(),
        (quarter.width * quarter.height * 4) as usize
    );
    // A region offset into the page reports a non-zero origin.
    let mid = b
        .render_region(0, 2.0, (pw / 2.0, ph / 2.0, pw, ph))
        .unwrap();
    assert!(mid.origin.0 > 0.0 && mid.origin.1 > 0.0);
}

#[test]
fn geometry_aligns_with_rendered_page_frame() {
    // Extracted geometry must live in the same [0,0,pw,ph] frame as the render
    // (CTM applied), not in centred/raw path space — else snapping is offset.
    let b = backend();
    let (pw, ph) = b.page_size_pts(0);
    let polylines = b.extract_geometry(0).unwrap();
    let (mut minx, mut miny, mut maxx, mut maxy) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for pl in &polylines {
        for v in pl.vertices() {
            minx = minx.min(v.x);
            miny = miny.min(v.y);
            maxx = maxx.max(v.x);
            maxy = maxy.max(v.y);
        }
    }
    assert!(
        minx > -5.0 && miny > -5.0,
        "geometry before origin: ({minx:.0},{miny:.0})"
    );
    assert!(
        maxx < pw + 5.0 && maxy < ph + 5.0,
        "geometry past page: ({maxx:.0},{maxy:.0}) vs ({pw:.0},{ph:.0})"
    );
    assert!(
        maxx - minx > pw * 0.5 && maxy - miny > ph * 0.5,
        "geometry too small"
    );
}

/// Optional smoke test against a real-world drawing: set `VERNIER_TEST_PDF`
/// to any CAD-exported plan to check extraction finds vector geometry in it.
#[test]
fn real_world_pdf_smoke() {
    let Ok(path) = std::env::var("VERNIER_TEST_PDF") else {
        eprintln!("VERNIER_TEST_PDF not set, skipping");
        return;
    };
    let b = MupdfBackend::open(&path).expect("open VERNIER_TEST_PDF");
    assert!(b.page_count() >= 1);
    let polylines = b.extract_geometry(0).unwrap();
    assert!(
        !polylines.is_empty(),
        "no vector geometry on page 0 of {path}"
    );
}
