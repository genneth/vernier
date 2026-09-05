// SPDX-License-Identifier: AGPL-3.0-or-later
//! The render service: a dedicated thread that OWNS the MuPDF document.
//!
//! The document is `!Send` (raw pointers), so it never crosses a thread
//! boundary — the thread opens it itself and only ever exchanges `Send` data
//! with the main thread: [`Req`] in over an mpsc channel, [`Resp`] out over an
//! async channel drained on the GTK main context. Rasterisation (tens to
//! ~150 ms) therefore never touches the UI loop.
//!
//! Requests carry a document id (`doc`) or page generation (`gen`) so the
//! receiver can drop results that belong to a document or page it has since
//! left behind.
use std::collections::VecDeque;
use std::sync::mpsc;

use vernier_core::geometry::{PagePt, PageRect, PageSize, Polyline};
use vernier_core::pdf::MupdfBackend;

/// Target width (px) for sidebar page thumbnails.
pub const THUMB_W: f64 = 180.0;
/// Target long-edge (px) for the coarse full-page preview.
const PREVIEW_LONG_EDGE: f64 = 1600.0;

pub enum Req {
    Open {
        doc: u64,
        path: String,
    },
    Geometry {
        gen: u64,
        page: usize,
    },
    Render {
        gen: u64,
        id: u64,
        page: usize,
        scale: f64,
        clip: PageRect,
    },
    Preview {
        gen: u64,
        page: usize,
    },
    Thumbnail {
        doc: u64,
        page: usize,
    },
}

/// A rendered RGBA8 image, ready to wrap as a texture.
pub struct Image {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub enum Resp {
    Opened {
        doc: u64,
        page_sizes: Vec<PageSize>,
    },
    Geometry {
        gen: u64,
        polylines: Vec<Polyline>,
    },
    Rendered {
        gen: u64,
        id: u64,
        image: Image,
        origin: PagePt,
        scale: f64,
    },
    Preview {
        gen: u64,
        image: Image,
    },
    Thumbnail {
        doc: u64,
        page: usize,
        image: Image,
    },
    /// A request failed. `what` names the operation for the user.
    Error {
        what: &'static str,
        message: String,
    },
}

fn image(img: vernier_core::pdf::Rgba) -> (Image, PagePt, f64) {
    (
        Image {
            bytes: img.bytes,
            width: img.width,
            height: img.height,
        },
        img.origin,
        img.scale,
    )
}

fn full_page(size: PageSize) -> PageRect {
    PageRect {
        x0: 0.0,
        y0: 0.0,
        x1: size.w,
        y1: size.h,
    }
}

/// Per-thread state.
struct Worker {
    backend: Option<MupdfBackend>,
    doc_id: u64,
    /// Thumbnails wait until no interactive work is pending.
    thumbs: VecDeque<usize>,
    out: async_channel::Sender<Resp>,
}

impl Worker {
    fn send(&self, resp: Resp) {
        // The receiver is gone only when the UI has been torn down.
        let _ = self.out.send_blocking(resp);
    }

    fn fail(&self, what: &'static str, e: anyhow::Error) {
        self.send(Resp::Error {
            what,
            message: format!("{e:#}"),
        });
    }

    fn process(&mut self, req: Req) {
        match req {
            Req::Open { doc, path } => {
                self.doc_id = doc;
                self.thumbs.clear(); // a new document invalidates pending thumbnails
                match Self::open(&path) {
                    Ok((b, page_sizes)) => {
                        self.backend = Some(b);
                        self.send(Resp::Opened { doc, page_sizes });
                    }
                    Err(e) => {
                        self.backend = None;
                        self.fail("Could not open the PDF", e);
                    }
                }
            }
            Req::Geometry { gen, page } => {
                let Some(b) = self.backend.as_ref() else {
                    return;
                };
                match b.extract_geometry(page) {
                    Ok(polylines) => self.send(Resp::Geometry { gen, polylines }),
                    Err(e) => self.fail("Could not read the page's vector geometry", e),
                }
            }
            Req::Render {
                gen,
                id,
                page,
                scale,
                clip,
            } => {
                let Some(b) = self.backend.as_ref() else {
                    return;
                };
                let t0 = std::time::Instant::now();
                match b.render_region(page, scale, clip) {
                    Ok(img) => {
                        tracing::debug!(
                            "rendered page {page} region {}x{} (scale {scale:.3}) in {:.1} ms",
                            img.width,
                            img.height,
                            t0.elapsed().as_secs_f64() * 1000.0
                        );
                        let (image, origin, scale) = image(img);
                        self.send(Resp::Rendered {
                            gen,
                            id,
                            image,
                            origin,
                            scale,
                        });
                    }
                    Err(e) => self.fail("Could not render the page", e),
                }
            }
            Req::Preview { gen, page } => {
                let Some(b) = self.backend.as_ref() else {
                    return;
                };
                // Whole page at a low fixed long-edge — a soft fallback layer.
                let result = b.page_size(page).and_then(|size| {
                    let long = size.w.max(size.h).max(1.0);
                    let scale = (PREVIEW_LONG_EDGE / long).clamp(0.05, 4.0);
                    b.render_region(page, scale, full_page(size))
                });
                match result {
                    Ok(img) => {
                        let (image, _, _) = image(img);
                        self.send(Resp::Preview { gen, image });
                    }
                    Err(e) => self.fail("Could not render the page preview", e),
                }
            }
            Req::Thumbnail { doc, page } => {
                // Queue only; rendered later when no interactive work is pending.
                if doc == self.doc_id {
                    self.thumbs.push_back(page);
                }
            }
        }
    }

    fn open(path: &str) -> anyhow::Result<(MupdfBackend, Vec<PageSize>)> {
        let b = MupdfBackend::open(path)?;
        let n = b.page_count()?;
        let sizes = (0..n)
            .map(|i| b.page_size(i))
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok((b, sizes))
    }

    fn render_thumbnail(&self, page: usize) {
        let Some(b) = self.backend.as_ref() else {
            return;
        };
        let result = b.page_size(page).and_then(|size| {
            let scale = (THUMB_W / size.w.max(1.0)).max(0.01);
            b.render_region(page, scale, full_page(size))
        });
        match result {
            Ok(img) => {
                let (image, _, _) = image(img);
                self.send(Resp::Thumbnail {
                    doc: self.doc_id,
                    page,
                    image,
                });
            }
            // Thumbnails are cosmetic: log, don't bother the user.
            Err(e) => tracing::warn!("thumbnail page {page}: {e:#}"),
        }
    }

    /// Serve interactive requests first; when the queue is idle, render one
    /// pending thumbnail at a time, re-checking for interactive work before
    /// each — so live zoom/pan always preempts thumbnail rendering. Within a
    /// queued batch, only the newest `Render` is kept (stale zoom levels are
    /// worthless); everything else runs in order.
    fn run(mut self, req_rx: mpsc::Receiver<Req>) {
        loop {
            let first = if self.thumbs.is_empty() {
                match req_rx.recv() {
                    Ok(r) => r,
                    Err(_) => break,
                }
            } else {
                match req_rx.try_recv() {
                    Ok(r) => r,
                    Err(mpsc::TryRecvError::Empty) => {
                        if let Some(page) = self.thumbs.pop_front() {
                            self.render_thumbnail(page);
                        }
                        continue;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => break,
                }
            };
            let mut batch = vec![first];
            while let Ok(more) = req_rx.try_recv() {
                batch.push(more);
            }
            let last_render = batch.iter().rposition(|r| matches!(r, Req::Render { .. }));
            for (i, req) in batch.into_iter().enumerate() {
                if matches!(req, Req::Render { .. }) && Some(i) != last_render {
                    continue; // superseded by a newer render in this batch
                }
                self.process(req);
            }
        }
    }
}

/// Start the render thread. Requests go in through the returned sender;
/// results come out of the receiver (drain it on the GTK main context).
pub fn spawn() -> (mpsc::Sender<Req>, async_channel::Receiver<Resp>) {
    let (req_tx, req_rx) = mpsc::channel::<Req>();
    let (resp_tx, resp_rx) = async_channel::unbounded::<Resp>();
    std::thread::Builder::new()
        .name("vernier-render".into())
        .spawn(move || {
            // Built here, on the thread that owns it: `MupdfBackend` is !Send.
            let worker = Worker {
                backend: None,
                doc_id: 0,
                thumbs: VecDeque::new(),
                out: resp_tx,
            };
            worker.run(req_rx)
        })
        .expect("spawn render thread");
    (req_tx, resp_rx)
}
