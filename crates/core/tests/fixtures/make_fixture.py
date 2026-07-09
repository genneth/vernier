#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Generate the synthetic test fixture `plan.pdf` (deterministic, stdlib-only).

A two-page A4 vector "floor plan": pure line/rect/bezier geometry, so the
MuPDF backend extracts a substantial, stable set of polylines for the
fixture tests. Regenerate with:  python3 make_fixture.py
"""

import io
import os

W, H = 595, 842  # A4 portrait, PDF points, y-up


class Stream:
    def __init__(self):
        self.b = io.StringIO()

    def op(self, s):
        self.b.write(s + "\n")

    def width(self, w):
        self.op(f"{w} w")

    def line(self, x1, y1, x2, y2):
        self.op(f"{x1} {y1} m {x2} {y2} l S")

    def rect(self, x, y, w, h):
        self.op(f"{x} {y} {w} {h} re S")

    def arc_q(self, cx, cy, r, quadrant):
        """Stroke a quarter-circle (door swing) via one bezier."""
        k = 0.5523 * r
        pts = {
            # start point, control offsets, end point per quadrant
            0: ((cx + r, cy), (cx + r, cy + k), (cx + k, cy + r), (cx, cy + r)),
            1: ((cx, cy + r), (cx - k, cy + r), (cx - r, cy + k), (cx - r, cy)),
            2: ((cx - r, cy), (cx - r, cy - k), (cx - k, cy - r), (cx, cy - r)),
            3: ((cx, cy - r), (cx + k, cy - r), (cx + r, cy - k), (cx + r, cy)),
        }[quadrant]
        (sx, sy), (c1x, c1y), (c2x, c2y), (ex, ey) = pts
        self.op(f"{sx} {sy} m {c1x} {c1y} {c2x} {c2y} {ex} {ey} c S")

    def text(self, x, y, size, s):
        self.op(f"BT /F1 {size} Tf {x} {y} Td ({s}) Tj ET")

    def bytes(self):
        return self.b.getvalue().encode("ascii")


def plan_page(title, upper):
    """One floor of the imaginary two-storey sample house."""
    s = Stream()

    # Outer walls: double line.
    s.width(2)
    s.rect(60, 80, 475, 680)
    s.width(1)
    s.rect(72, 92, 451, 656)

    # Interior partitions (double lines, 8pt wall thickness).
    def wall_h(x1, x2, y):
        s.line(x1, y, x2, y)
        s.line(x1, y + 8, x2, y + 8)

    def wall_v(x, y1, y2):
        s.line(x, y1, x, y2)
        s.line(x + 8, y1, x + 8, y2)

    s.width(1.5)
    wall_h(72, 320, 500)          # living / kitchen divide
    wall_h(380, 523, 500)
    wall_v(300, 92, 380)          # hall wall
    wall_h(72, 240, 300)
    wall_v(220, 500, 748)
    if upper:
        wall_h(150, 523, 640)     # extra bedroom divide upstairs

    # Doors: opening gap + quarter-circle swing.
    s.width(0.8)
    s.arc_q(320, 500, 60, 1)
    s.arc_q(300, 340, 55, 0)
    s.arc_q(220, 560, 50, 3)

    # Stair: 13 treads.
    for i in range(13):
        y = 110 + i * 18
        s.line(430, y, 510, y)
    s.line(430, 110, 430, 110 + 12 * 18)
    s.line(510, 110, 510, 110 + 12 * 18)

    # Kitchen counter / fixtures (small rects).
    s.rect(80, 700, 40, 40)
    s.rect(130, 700, 40, 40)
    s.rect(80, 640, 40, 40)
    s.rect(450, 700, 60, 45)
    s.rect(85, 300, 70, 50)

    # Terrace decking hatch: diagonals.
    s.width(0.5)
    for i in range(14):
        x = 340 + i * 13
        s.line(x, 96, min(x + 60, 519), min(96 + 60, 156))

    # Survey grid ticks along the bottom and left edges.
    for i in range(20):
        x = 70 + i * 24
        s.line(x, 66, x, 74)
    for i in range(28):
        y = 90 + i * 24
        s.line(52, y, 60, y)

    # Title block.
    s.width(1)
    s.rect(340, 20, 195, 34)
    s.text(348, 40, 10, "SAMPLE HOUSE - " + title)
    s.text(348, 27, 7, "Synthetic fixture. Not a real building. 1:50 at A4.")
    s.text(66, 764, 8, "vernier test fixture")
    return s.bytes()


def build_pdf(path):
    pages = [plan_page("GROUND FLOOR", False), plan_page("FIRST FLOOR", True)]
    objs = {}
    objs[1] = b"<< /Type /Catalog /Pages 2 0 R >>"
    objs[2] = b"<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>"
    for i, content in enumerate(pages):
        page_num, cont_num = 3 + 2 * i, 4 + 2 * i
        objs[page_num] = (
            f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {W} {H}] "
            f"/Contents {cont_num} 0 R /Resources << /Font << /F1 7 0 R >> >> >>"
        ).encode("ascii")
        objs[cont_num] = (
            f"<< /Length {len(content)} >>\nstream\n".encode("ascii")
            + content
            + b"endstream"
        )
    objs[7] = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"

    out = io.BytesIO()
    out.write(b"%PDF-1.4\n")
    offsets = {}
    for n in sorted(objs):
        offsets[n] = out.tell()
        out.write(f"{n} 0 obj\n".encode("ascii") + objs[n] + b"\nendobj\n")
    xref = out.tell()
    out.write(f"xref\n0 {len(objs) + 1}\n".encode("ascii"))
    out.write(b"0000000000 65535 f \n")
    for n in sorted(objs):
        out.write(f"{offsets[n]:010d} 00000 n \n".encode("ascii"))
    out.write(
        (
            f"trailer\n<< /Size {len(objs) + 1} /Root 1 0 R >>\n"
            f"startxref\n{xref}\n%%EOF\n"
        ).encode("ascii")
    )
    with open(path, "wb") as f:
        f.write(out.getvalue())
    print(f"wrote {path} ({out.tell()} bytes)")


if __name__ == "__main__":
    build_pdf(os.path.join(os.path.dirname(os.path.abspath(__file__)), "plan.pdf"))
