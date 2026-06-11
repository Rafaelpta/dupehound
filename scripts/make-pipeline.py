#!/usr/bin/env python3
"""Generates assets/pipeline.svg: the pipeline diagram, Excalidraw-style.

Hand-laid-out boxes with Excalidraw's pastel palette and its Virgil font
embedded as base64 (OFL). Pass the font path as argv[1] if not in /tmp.
"""

import base64
import sys

FONT_PATH = sys.argv[1] if len(sys.argv) > 1 else "/tmp/Virgil.woff2"
font = base64.b64encode(open(FONT_PATH, "rb").read()).decode()

BLUE_F, BLUE_S = "#e7f5ff", "#1971c2"
YELL_F, YELL_S = "#fff9db", "#f08c00"
MINT_F, MINT_S = "#ebfbee", "#2f9e44"
INK = "#1e1e1e"
GRAY = "#868e96"


def rrect(x, y, w, h, fill, stroke, rx=14, sw=1.6):
    return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" '
            f'fill="{fill}" stroke="{stroke}" stroke-width="{sw}"/>')


def text(x, y, s, size=15, fill=INK, anchor="middle"):
    return (f'<text x="{x}" y="{y}" font-family="Virgil" font-size="{size}" '
            f'fill="{fill}" text-anchor="{anchor}">{s}</text>')


def pill(cx, y, w, label, stroke, h=30):
    return rrect(cx - w / 2, y, w, h, "#ffffff", stroke, rx=10) + text(cx, y + 20, label, 14)


def arrow(x1, y1, x2, y2):
    return (f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{INK}" stroke-width="1.8"/>'
            f'<path d="M {x2} {y2} l -10 -5 l 2 5 l -2 5 z" fill="{INK}"/>')


parts = [f"""<svg xmlns="http://www.w3.org/2000/svg" width="1150" height="430" viewBox="0 0 1150 430">
<defs><style>@font-face{{font-family:'Virgil';src:url(data:font/woff2;base64,{font}) format('woff2');}}</style></defs>
<rect width="1150" height="430" fill="white"/>"""]

bx, by, bw, bh = 30, 130, 190, 130
parts += [rrect(bx, by, bw, bh, BLUE_F, BLUE_S, rx=18, sw=2),
          text(bx + 18, by + 26, "Discover", 16, BLUE_S, "start"),
          text(bx + bw - 14, by + 24, "step 1", 11, GRAY, "end"),
          pill(bx + bw / 2, by + 42, 150, "walk the repo", BLUE_S),
          pill(bx + bw / 2, by + 80, 150, "skip generated", BLUE_S),
          text(bx + bw / 2, by + bh + 20, "gitignore-aware", 12, GRAY),
          arrow(bx + bw + 8, by + bh / 2, bx + bw + 44, by + bh / 2)]

fx, fy, fw, fh = 274, 48, 330, 300
parts += [rrect(fx, fy, fw, fh, YELL_F, YELL_S, rx=20, sw=2),
          text(fx + 18, fy + 28, "Fingerprint", 17, "#b07b02", "start"),
          text(fx + fw - 16, fy + 26, "step 2", 11, GRAY, "end")]
g1x, g1y, g1w, g1h = fx + 18, fy + 44, 142, 128
parts += [rrect(g1x, g1y, g1w, g1h, "#fff4cc", YELL_S, rx=14),
          text(g1x + g1w / 2, g1y + 22, "parse", 13, "#b07b02"),
          pill(g1x + g1w / 2, g1y + 34, 118, "tree-sitter", YELL_S, 28),
          pill(g1x + g1w / 2, g1y + 70, 118, "function bodies", YELL_S, 28)]
g2x, g2y, g2w, g2h = fx + fw - 160, fy + 44, 142, 240
parts += [rrect(g2x, g2y, g2w, g2h, "#fff4cc", YELL_S, rx=14),
          text(g2x + g2w / 2, g2y + 22, "normalize", 13, "#b07b02"),
          pill(g2x + g2w / 2, g2y + 34, 118, "ids &#8594; ID", YELL_S, 28),
          pill(g2x + g2w / 2, g2y + 70, 118, "literals &#8594; LIT", YELL_S, 28),
          pill(g2x + g2w / 2, g2y + 106, 118, "drop comments", YELL_S, 28),
          pill(g2x + g2w / 2, g2y + 146, 118, "10-gram hashes", YELL_S, 28),
          pill(g2x + g2w / 2, g2y + 182, 118, "winnowing", YELL_S, 28),
          text(fx + fw / 2, fy + fh + 20, "per function, in parallel (MOSS, SIGMOD 2003)", 12, GRAY),
          arrow(fx + fw + 8, fy + fh / 2 - 46, fx + fw + 44, fy + fh / 2 - 46)]

mx, my, mw, mh = 658, 88, 210, 220
parts += [rrect(mx, my, mw, mh, MINT_F, MINT_S, rx=18, sw=2),
          text(mx + 18, my + 26, "Match", 16, MINT_S, "start"),
          text(mx + mw - 14, my + 24, "step 3", 11, GRAY, "end"),
          pill(mx + mw / 2, my + 42, 166, "inverted index", MINT_S),
          pill(mx + mw / 2, my + 82, 166, "cull boilerplate", MINT_S),
          pill(mx + mw / 2, my + 122, 166, "jaccard similarity", MINT_S),
          pill(mx + mw / 2, my + 162, 166, "union-find clusters", MINT_S),
          text(mx + mw / 2, my + mh + 20, "no O(n&#178;) pass", 12, GRAY),
          arrow(mx + mw + 8, my + mh / 2, mx + mw + 44, my + mh / 2)]

rx_, ry, rw, rh = 922, 128, 196, 140
parts += [rrect(rx_, ry, rw, rh, MINT_F, MINT_S, rx=18, sw=2),
          text(rx_ + 18, ry + 26, "Report", 16, MINT_S, "start"),
          text(rx_ + rw - 14, ry + 24, "step 4", 11, GRAY, "end"),
          pill(rx_ + rw / 2, ry + 42, 152, "slop score", MINT_S),
          pill(rx_ + rw / 2, ry + 82, 152, "exit 1 in CI", MINT_S),
          text(rx_ + rw / 2, ry + rh + 20, "scan &#183; history &#183; check", 12, GRAY),
          "</svg>"]

open("assets/pipeline.svg", "w").write("\n".join(parts))
print("assets/pipeline.svg written")
