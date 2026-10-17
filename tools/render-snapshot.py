#!/usr/bin/env python3
"""Render a TypeWords TUI snapshot (JSON from tui/examples/shot.rs) to a PNG.

The JSON contract (produced by ``cargo run --example shot``)::

    {
      "width": 120,
      "height": 40,
      "screen": "home",          # optional metadata, used for default file name
      "theme": "dark",           # optional metadata, used for default file name
      "rows": [
        [
          {"sym": " ", "fg": "reset", "bg": "reset", "bold": false,
           "reversed": false, "underline": false, "width": 1},
          ...
        ],
        ...
      ]
    }

Colors are serialized as ``"reset"``, a named ANSI color (``black``, ``red``,
``darkgray``, ``lightcyan`` ...), ``"#rrggbb"`` or ``"indexed:N"``.
A cell whose ``width`` is 2 (wide CJK glyph) makes the renderer skip the next
cell(s) of that row, which are the wide character's continuation cells.

Fonts: Cascadia Mono (or another monospace font) is used for ASCII / box
drawing / block glyphs, Microsoft YaHei (or another CJK font) for CJK.  Box
drawing and block-element glyphs used by the TUI are drawn procedurally so
that borders line up crisply; everything else falls back to the real fonts.
When a glyph is missing from the primary font the CJK font is tried.

Common usage::

    # bundled Python (Pillow) from the DSH runtime, defaults: scale 2, dark theme
    python tui/tools/render-snapshot.py %TEMP%\\home.json
    # -> tui/target/screenshots/home-120x40-dark.png

    python tui/tools/render-snapshot.py %TEMP%\\home.json out\\home.png --scale 3
    python tui/tools/render-snapshot.py - --out snap.png < snap.json

Only Pillow is required (Pillow >= 9, tested with 12.3).
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:  # pragma: no cover - the bundled runtime always has Pillow
    sys.stderr.write("error: Pillow is required (pip install Pillow)\n")
    raise SystemExit(2)


# ---------------------------------------------------------------------------
# Colors
# ---------------------------------------------------------------------------

# Basic ANSI palette (0-15), roughly the VS Code / xterm "campbell"-like set so
# that named colours and indexed 0-15 agree.
ANSI_PALETTE = [
    (0x00, 0x00, 0x00),  # black
    (0xCD, 0x31, 0x31),  # red
    (0x0D, 0xBC, 0x79),  # green
    (0xE5, 0xE5, 0x10),  # yellow
    (0x24, 0x72, 0xC8),  # blue
    (0xBC, 0x3F, 0xBC),  # magenta
    (0x11, 0xA8, 0xCD),  # cyan
    (0xE5, 0xE5, 0xE5),  # gray / white (ANSI 7)
    (0x66, 0x66, 0x66),  # dark gray (ANSI 8)
    (0xF1, 0x4C, 0x4C),  # light red
    (0x23, 0xD1, 0x8B),  # light green
    (0xF5, 0xF5, 0x43),  # light yellow
    (0x3B, 0x8E, 0xEA),  # light blue
    (0xD6, 0x70, 0xD6),  # light magenta
    (0x29, 0xB8, 0xDB),  # light cyan
    (0xFF, 0xFF, 0xFF),  # white
]

NAMED_COLORS = {
    "black": ANSI_PALETTE[0],
    "red": ANSI_PALETTE[1],
    "green": ANSI_PALETTE[2],
    "yellow": ANSI_PALETTE[3],
    "blue": ANSI_PALETTE[4],
    "magenta": ANSI_PALETTE[5],
    "cyan": ANSI_PALETTE[6],
    "gray": ANSI_PALETTE[7],
    "grey": ANSI_PALETTE[7],
    "darkgray": ANSI_PALETTE[8],
    "darkgrey": ANSI_PALETTE[8],
    "dark_gray": ANSI_PALETTE[8],
    "dark_grey": ANSI_PALETTE[8],
    "lightred": ANSI_PALETTE[9],
    "light_red": ANSI_PALETTE[9],
    "lightgreen": ANSI_PALETTE[10],
    "light_green": ANSI_PALETTE[10],
    "lightyellow": ANSI_PALETTE[11],
    "light_yellow": ANSI_PALETTE[11],
    "lightblue": ANSI_PALETTE[12],
    "light_blue": ANSI_PALETTE[12],
    "lightmagenta": ANSI_PALETTE[13],
    "light_magenta": ANSI_PALETTE[13],
    "lightcyan": ANSI_PALETTE[14],
    "light_cyan": ANSI_PALETTE[14],
    "white": ANSI_PALETTE[15],
    "brightblack": ANSI_PALETTE[8],
    "bright_black": ANSI_PALETTE[8],
    "brightred": ANSI_PALETTE[9],
    "brightgreen": ANSI_PALETTE[10],
    "brightyellow": ANSI_PALETTE[11],
    "brightblue": ANSI_PALETTE[12],
    "brightmagenta": ANSI_PALETTE[13],
    "brightcyan": ANSI_PALETTE[14],
    "brightwhite": ANSI_PALETTE[15],
}

THEME_DEFAULTS = {
    # default foreground/background when a cell says "reset" (terminal default)
    "dark": {"fg": (0xD0, 0xD0, 0xD8), "bg": (0x17, 0x17, 0x20)},
    "light": {"fg": (0x20, 0x20, 0x24), "bg": (0xFA, 0xFA, 0xF8)},
    "high-contrast": {"fg": (0xFF, 0xFF, 0xFF), "bg": (0x00, 0x00, 0x00)},
    "highcontrast": {"fg": (0xFF, 0xFF, 0xFF), "bg": (0x00, 0x00, 0x00)},
}
UNKNOWN_THEME_DEFAULT = {"fg": (0xD0, 0xD0, 0xD8), "bg": (0x17, 0x17, 0x20)}


def indexed_color(index: int):
    index = max(0, min(255, int(index)))
    if index < 16:
        return ANSI_PALETTE[index]
    if index < 232:
        index -= 16
        r, g, b = index // 36, (index % 36) // 6, index % 6
        steps = (0, 95, 135, 175, 215, 255)
        return (steps[r], steps[g], steps[b])
    level = 8 + (index - 232) * 10
    return (level, level, level)


def parse_color(value):
    """Return an (r, g, b) tuple, or None for the terminal default color."""
    if value is None:
        return None
    if not isinstance(value, str):
        raise ValueError(f"color must be a string, got {value!r}")
    raw = value.strip().lower().replace(" ", "")
    if raw in ("", "reset", "default", "none", "terminal"):
        return None
    if raw.startswith("#"):
        digits = raw[1:]
        if len(digits) == 3:
            digits = "".join(ch * 2 for ch in digits)
        if len(digits) not in (6, 8):
            raise ValueError(f"bad hex color: {value!r}")
        return (int(digits[0:2], 16), int(digits[2:4], 16), int(digits[4:6], 16))
    if raw.startswith("indexed:"):
        return indexed_color(int(raw.split(":", 1)[1]))
    if raw in NAMED_COLORS:
        return NAMED_COLORS[raw]
    match = re.fullmatch(r"rgb\((\d+),(\d+),(\d+)\)", raw)
    if match:
        return tuple(max(0, min(255, int(part))) for part in match.groups())
    raise ValueError(f"unknown color: {value!r}")


def theme_palette(theme: str):
    key = (theme or "").strip().lower()
    return THEME_DEFAULTS.get(key, UNKNOWN_THEME_DEFAULT)


# ---------------------------------------------------------------------------
# Fonts
# ---------------------------------------------------------------------------

MONO_FONT_NAMES = [
    "CascadiaMono.ttf",
    "CascadiaCode.ttf",
    "CascadiaMonoPL.ttf",
    "consola.ttf",
    "lucon.ttf",
    "DejaVuSansMono.ttf",
    "JetBrainsMono-Regular.ttf",
    "JetBrainsMono[wght].ttf",
    "UbuntuMono-R.ttf",
    "UbuntuMono[wght].ttf",
    "LiberationMono-Regular.ttf",
    "NotoSansMono-Regular.ttf",
    "SourceCodePro-Regular.ttf",
    "FiraCode-Regular.ttf",
    "Menlo.ttc",
    "Consolas.ttf",
]

CJK_FONT_NAMES = [
    "msyh.ttc",  # Microsoft YaHei
    "msyh.ttf",
    "msyhbd.ttc",
    "simhei.ttf",
    "simsun.ttc",
    "NotoSansSC-VF.ttf",
    "NotoSansSC-Regular.otf",
    "NotoSansCJK-Regular.ttc",
    "NotoSansCJKsc-Regular.otf",
    "SourceHanSansSC-Regular.otf",
    "wqy-zenhei.ttc",
    "wqy-microhei.ttc",
    "DroidSansFallbackFull.ttf",
    "PingFang.ttc",
    "Hiragino Sans GB.ttc",
    "STHeiti Light.ttc",
    "Arial Unicode.ttf",
]


def _font_directories():
    dirs = []
    local_appdata = os.environ.get("LOCALAPPDATA")
    if local_appdata:
        dirs.append(Path(local_appdata) / "Microsoft" / "Windows" / "Fonts")
    windir = os.environ.get("WINDIR") or os.environ.get("SystemRoot")
    if windir:
        dirs.append(Path(windir) / "Fonts")
    home = Path.home()
    dirs.extend(
        [
            home / ".fonts",
            home / ".local" / "share" / "fonts",
            Path("/usr/share/fonts"),
            Path("/usr/local/share/fonts"),
            Path("/System/Library/Fonts"),
            Path("/Library/Fonts"),
        ]
    )
    return [path for path in dirs if path.exists()]


def _find_font(names, extra_dirs=(), recursive_roots=()):
    """Find the first existing font file for one of the given file names."""
    dirs = list(extra_dirs) + _font_directories()
    for name in names:
        for directory in dirs:
            candidate = directory / name
            if candidate.is_file():
                return candidate
            # On Linux fonts are nested in package directories.
            if directory in recursive_roots:
                matches = sorted(directory.rglob(name))
                if matches:
                    return matches[0]
    return None


def resolve_fonts(mono_path, cjk_path):
    env_mono = os.environ.get("SHOT_FONT") or os.environ.get("TYPEWORDS_SHOT_FONT")
    env_cjk = os.environ.get("SHOT_CJK_FONT") or os.environ.get("TYPEWORDS_SHOT_CJK_FONT")
    if mono_path is None and env_mono:
        mono_path = Path(env_mono)
    if cjk_path is None and env_cjk:
        cjk_path = Path(env_cjk)
    if mono_path is not None and not Path(mono_path).is_file():
        raise SystemExit(f"error: mono font not found: {mono_path}")
    if cjk_path is not None and not Path(cjk_path).is_file():
        raise SystemExit(f"error: CJK font not found: {cjk_path}")
    if mono_path is None:
        mono_path = _find_font(
            MONO_FONT_NAMES,
            recursive_roots=[Path("/usr/share/fonts"), Path("/usr/local/share/fonts")],
        )
    if cjk_path is None:
        cjk_path = _find_font(
            CJK_FONT_NAMES,
            recursive_roots=[Path("/usr/share/fonts"), Path("/usr/local/share/fonts")],
        )
    if mono_path is None:
        raise SystemExit(
            "error: no monospace font found; pass --font <path> "
            "(tried Cascadia Mono, Consolas, DejaVu Sans Mono, ...)"
        )
    return Path(mono_path), (Path(cjk_path) if cjk_path else Path(mono_path))


class GlyphRenderer:
    """Draws individual terminal cells and caches font-related work."""

    def __init__(self, mono_path: Path, cjk_path: Path, scale: int,
                 font_size=None, cell_width=None, cell_height=None):
        self.scale = max(1, int(scale))
        self.cell_w = int(cell_width) if cell_width else 8 * self.scale
        self.cell_h = int(cell_height) if cell_height else 16 * self.scale
        self.mono_path = mono_path
        self.cjk_path = cjk_path
        self.mono = self._load_font(mono_path, font_size, probe="M", max_advance=self.cell_w)
        if cjk_path and cjk_path != mono_path:
            # A fullwidth CJK glyph must fit into two cells; keep it the same
            # pixel size as the monospace font (do not fit it to one cell).
            self.cjk = self._load_font(cjk_path, font_size, probe="中",
                                       max_advance=2 * self.cell_w)
        else:
            self.cjk = self.mono
        self._coverage = {}
        self._ref_mask = {}
        # Vertical baseline: centre the monospace font's line box in the cell.
        ascent, descent = self.mono.getmetrics()
        self.baseline = max(1, (self.cell_h - (ascent + descent)) // 2 + ascent)
        # Bold text uses a stroke so it stays legible at scale 1.
        self.bold_stroke = max(1, self.scale // 2)
        self.line_width = max(1, round(self.scale * 0.8))

    # -- font loading -----------------------------------------------------
    def _load_font(self, path: Path, explicit_size, probe="M", max_advance=None):
        if max_advance is None:
            max_advance = self.cell_w
        if explicit_size:
            return ImageFont.truetype(str(path), int(explicit_size))
        # Shrink until the probe glyph fits the available horizontal space.
        size = max(6, int(self.cell_h * 0.85))
        for _ in range(10):
            font = ImageFont.truetype(str(path), size)
            if font.getlength(probe) <= max_advance + 0.01:
                return font
            size -= 1
        return ImageFont.truetype(str(path), max(6, size))

    # -- glyph coverage ---------------------------------------------------
    def _render_mask(self, font, text):
        canvas = Image.new("L", (font.size * 3, font.size * 3), 0)
        ImageDraw.Draw(canvas).text((0, 0), text, font=font, fill=255, anchor="la")
        return canvas.tobytes()

    def font_has(self, font, character: str) -> bool:
        key = (id(font), character)
        cached = self._coverage.get(key)
        if cached is not None:
            return cached
        ref = self._ref_mask.get(id(font))
        if ref is None:
            ref = self._render_mask(font, "\u0378")  # unassigned -> .notdef
            self._ref_mask[id(font)] = ref
        result = self._render_mask(font, character) != ref
        self._coverage[key] = result
        return result

    def pick_font(self, character: str):
        """Choose the font that actually contains this character."""
        if self.cjk_path != self.mono_path and self._is_cjk(character):
            if self.font_has(self.cjk, character):
                return self.cjk
        if self.font_has(self.mono, character):
            return self.mono
        if self.cjk_path != self.mono_path and self.font_has(self.cjk, character):
            return self.cjk
        return self.mono

    @staticmethod
    def _is_cjk(character: str) -> bool:
        code = ord(character)
        return (
            0x2E80 <= code <= 0x9FFF
            or 0xAC00 <= code <= 0xD7AF
            or 0xF900 <= code <= 0xFAFF
            or 0xFE30 <= code <= 0xFE4F
            or 0xFF00 <= code <= 0xFFEF
            or 0x20000 <= code <= 0x3FFFF
        )

    # -- public drawing ---------------------------------------------------
    def draw_cell(self, draw, x, y, width_cells, symbol, fg, bg,
                  bold=False, reversed_=False, underline=False):
        """Draw one logical cell (width_cells >= 1) at grid position (x, y)."""
        x_px, y_px = x * self.cell_w, y * self.cell_h
        cell_px_w = width_cells * self.cell_w

        if reversed_:
            fg, bg = bg, fg
        if bg is not None:
            draw.rectangle(
                [x_px, y_px, x_px + cell_px_w - 1, y_px + self.cell_h - 1],
                fill=bg,
            )

        glyph = symbol or ""
        if glyph.strip() == "":
            if underline:
                self._draw_underline(draw, x_px, y_px, cell_px_w, fg)
            return
        if len(glyph) == 1 and self._draw_special(draw, x_px, y_px, cell_px_w, glyph, fg):
            if underline:
                self._draw_underline(draw, x_px, y_px, cell_px_w, fg)
            return

        font = self.pick_font(glyph[0])
        draw.text(
            (x_px, y_px + self.baseline),
            glyph,
            font=font,
            fill=fg,
            anchor="ls",
            stroke_width=self.bold_stroke if bold else 0,
            stroke_fill=fg if bold else None,
        )
        if underline:
            self._draw_underline(draw, x_px, y_px, cell_px_w, fg)

    def _draw_underline(self, draw, x_px, y_px, width_px, fg):
        width = max(1, self.scale // 2)
        yy = y_px + min(self.cell_h - width, self.baseline + max(1, self.scale))
        draw.rectangle([x_px, yy, x_px + width_px - 1, yy + width - 1], fill=fg)

    # -- procedural glyphs ------------------------------------------------
    def _draw_special(self, draw, x_px, y_px, width_px, character, color) -> bool:
        code = ord(character)
        if 0x2580 <= code <= 0x259F:  # block elements
            self._draw_block(draw, x_px, y_px, width_px, character, color)
            return True
        if code in BOX_DRAWING:
            up, down, left, right, style = BOX_DRAWING[code]
            self._draw_box(draw, x_px, y_px, width_px, up, down, left, right, style, color)
            return True
        return False

    def _draw_box(self, draw, x_px, y_px, width_px, up, down, left, right, style, color):
        cw, ch = width_px, self.cell_h
        cx, cy = x_px + cw / 2.0, y_px + ch / 2.0
        heavy = style == "heavy"
        line_w = self.line_width * (2 if heavy else 1)
        line_w = max(1, line_w)
        rounded = style == "rounded"
        radius = max(2, min(cw, ch) // 4)

        if rounded:
            if right and down:
                self._arc(draw, cx + radius, cy + radius, radius, 180, 270, line_w, color)
                draw.line([(cx + radius, cy), (x_px + cw, cy)], fill=color, width=line_w)
                draw.line([(cx, cy + radius), (cx, y_px + ch)], fill=color, width=line_w)
            elif left and down:
                self._arc(draw, cx - radius, cy + radius, radius, 270, 360, line_w, color)
                draw.line([(x_px, cy), (cx - radius, cy)], fill=color, width=line_w)
                draw.line([(cx, cy + radius), (cx, y_px + ch)], fill=color, width=line_w)
            elif up and right:
                self._arc(draw, cx + radius, cy - radius, radius, 90, 180, line_w, color)
                draw.line([(cx + radius, cy), (x_px + cw, cy)], fill=color, width=line_w)
                draw.line([(cx, y_px), (cx, cy - radius)], fill=color, width=line_w)
            else:  # up + left
                self._arc(draw, cx - radius, cy - radius, radius, 0, 90, line_w, color)
                draw.line([(x_px, cy), (cx - radius, cy)], fill=color, width=line_w)
                draw.line([(cx, y_px), (cx, cy - radius)], fill=color, width=line_w)
            return

        if left:
            draw.line([(x_px, cy), (cx, cy)], fill=color, width=line_w)
        if right:
            draw.line([(cx, cy), (x_px + cw, cy)], fill=color, width=line_w)
        if up:
            draw.line([(cx, y_px), (cx, cy)], fill=color, width=line_w)
        if down:
            draw.line([(cx, cy), (cx, y_px + ch)], fill=color, width=line_w)

    @staticmethod
    def _arc(draw, cx, cy, radius, start, end, width, color):
        draw.arc(
            [cx - radius, cy - radius, cx + radius, cy + radius],
            start=start,
            end=end,
            fill=color,
            width=width,
        )

    def _draw_block(self, draw, x_px, y_px, width_px, character, color):
        cw, ch = width_px, self.cell_h
        left, right, top, bottom = x_px, x_px + cw, y_px, y_px + ch

        def rect(x0, y0, x1, y1):
            x0, x1 = sorted((round(x0), round(x1)))
            y0, y1 = sorted((round(y0), round(y1)))
            if x1 > x0 and y1 > y0:
                draw.rectangle([x0, y0, x1 - 1, y1 - 1], fill=color)

        def frac(top_f, bottom_f=None, left_f=0.0, right_f=1.0):
            bottom_f = top_f if bottom_f is None else bottom_f
            rect(left + left_f * cw, top + top_f * ch,
                 left + right_f * cw, top + bottom_f * ch)

        if character == "\u2588":  # full block
            rect(left, top, right, bottom)
        elif character == "\u2580":
            frac(0.0, 0.5)
        elif character == "\u2584":
            frac(0.5, 1.0)
        elif character == "\u258c":
            frac(0.0, 1.0, 0.0, 0.5)
        elif character == "\u2590":
            frac(0.0, 1.0, 0.5, 1.0)
        elif character == "\u2581":
            frac(7 / 8, 1.0)
        elif character == "\u2582":
            frac(3 / 4, 1.0)
        elif character == "\u2583":
            frac(5 / 8, 1.0)
        elif character == "\u2585":
            frac(3 / 8, 1.0)
        elif character == "\u2586":
            frac(1 / 4, 1.0)
        elif character == "\u2587":
            frac(1 / 8, 1.0)
        elif character == "\u2594":
            frac(0.0, 1 / 8)
        elif character == "\u2589":
            frac(0.0, 1.0, 0.0, 7 / 8)
        elif character == "\u258a":
            frac(0.0, 1.0, 0.0, 3 / 4)
        elif character == "\u258b":
            frac(0.0, 1.0, 0.0, 5 / 8)
        elif character == "\u258d":
            frac(0.0, 1.0, 0.0, 3 / 8)
        elif character == "\u258e":
            frac(0.0, 1.0, 0.0, 1 / 4)
        elif character == "\u258f":
            frac(0.0, 1.0, 0.0, 1 / 8)
        elif character == "\u2595":
            frac(0.0, 1.0, 7 / 8, 1.0)
        elif character == "\u2591":  # light shade
            self._shade(draw, left, top, right, bottom, color, (1, 0, 0, 0))
        elif character == "\u2592":  # medium shade
            self._shade(draw, left, top, right, bottom, color, (1, 0, 1, 0))
        elif character == "\u2593":  # dark shade
            self._shade(draw, left, top, right, bottom, color, (1, 1, 1, 0))
        else:
            self._draw_quadrant(draw, left, top, right, bottom, character, color)

    def _shade(self, draw, left, top, right, bottom, color, rows):
        """Dither pattern; rows describes which of the four 2x2 sub-pixels."""
        step = max(2, self.scale)
        dot = max(1, self.scale // 2)
        for yy in range(round(top), round(bottom), step):
            for xx in range(round(left), round(right), step):
                half = step / 2.0
                dots = []
                if rows[0]:
                    dots.append((xx, yy))
                if rows[1]:
                    dots.append((xx + half, yy))
                if rows[2]:
                    dots.append((xx, yy + half))
                if rows[3]:
                    dots.append((xx + half, yy + half))
                for dot_x, dot_y in dots:
                    x1, y1 = int(dot_x), int(dot_y)
                    draw.rectangle(
                        [x1, y1, min(x1 + dot - 1, round(right) - 1),
                         min(y1 + dot - 1, round(bottom) - 1)],
                        fill=color,
                    )

    def _draw_quadrant(self, draw, left, top, right, bottom, character, color):
        masks = {
            "\u2596": ("ll",),                    # lower-left quadrant
            "\u2597": ("lr",),                    # lower-right quadrant
            "\u2598": ("ul",),                    # upper-left quadrant
            "\u2599": ("ul", "ll", "lr"),         # upper-left + lower quadrants
            "\u259a": ("ul", "lr"),               # upper-left + lower-right
            "\u259b": ("ul", "ur", "ll"),         # upper-left + upper-right + lower-left
            "\u259c": ("ul", "ur", "lr"),         # upper-left + upper-right + lower-right
            "\u259d": ("ur",),                    # upper-right quadrant
            "\u259e": ("ur", "ll"),               # upper-right + lower-left
            "\u259f": ("ur", "ll", "lr"),         # upper-right + lower quadrants
        }
        parts = masks.get(character)
        if not parts:
            return
        mx, my = (left + right) / 2.0, (top + bottom) / 2.0
        regions = {
            "ul": (left, top, mx, my),
            "ur": (mx, top, right, my),
            "ll": (left, my, mx, bottom),
            "lr": (mx, my, right, bottom),
        }
        for part in parts:
            x0, y0, x1, y1 = regions[part]
            draw.rectangle([round(x0), round(y0), round(x1) - 1, round(y1) - 1], fill=color)


# Direction table for the box-drawing subset we draw ourselves.
# value: (up, down, left, right, style) with style in single/rounded/heavy
BOX_DRAWING = {
    0x2500: (0, 0, 1, 1, "single"),
    0x2502: (1, 1, 0, 0, "single"),
    0x250C: (0, 1, 0, 1, "single"),
    0x2510: (0, 1, 1, 0, "single"),
    0x2514: (1, 0, 0, 1, "single"),
    0x2518: (1, 0, 1, 0, "single"),
    0x251C: (1, 1, 0, 1, "single"),
    0x2524: (1, 1, 1, 0, "single"),
    0x252C: (0, 1, 1, 1, "single"),
    0x2534: (1, 0, 1, 1, "single"),
    0x253C: (1, 1, 1, 1, "single"),
    0x256D: (0, 1, 0, 1, "rounded"),
    0x256E: (0, 1, 1, 0, "rounded"),
    0x256F: (1, 0, 1, 0, "rounded"),
    0x2570: (1, 0, 0, 1, "rounded"),
}


# ---------------------------------------------------------------------------
# Snapshot model
# ---------------------------------------------------------------------------

def load_snapshot(path):
    if path == "-":
        raw = sys.stdin.read()
        source = "<stdin>"
    else:
        source = path
        with open(path, "r", encoding="utf-8") as handle:
            raw = handle.read()
    try:
        data = json.loads(raw)
    except json.JSONDecodeError as error:
        raise SystemExit(f"error: {source} is not valid JSON: {error}")
    if not isinstance(data, dict):
        raise SystemExit(f"error: {source} must contain a JSON object")
    return data, source


def normalize_snapshot(data):
    try:
        width = int(data["width"])
        height = int(data["height"])
        rows = data["rows"]
    except KeyError as error:
        raise SystemExit(f"error: snapshot is missing key {error}")
    if width <= 0 or height <= 0:
        raise SystemExit(f"error: invalid snapshot size {width}x{height}")
    if not isinstance(rows, list):
        raise SystemExit("error: snapshot 'rows' must be a list")

    blank = {"sym": " ", "fg": "reset", "bg": "reset", "bold": False,
             "reversed": False, "underline": False, "width": 1}
    normalized_rows = []
    for y in range(height):
        raw_row = rows[y] if y < len(rows) else []
        if not isinstance(raw_row, list):
            raw_row = []
        row = []
        for x in range(width):
            cell = raw_row[x] if x < len(raw_row) else blank
            if not isinstance(cell, dict):
                cell = blank
            try:
                cell_width = int(cell.get("width") or 1)
            except (TypeError, ValueError):
                cell_width = 1
            row.append(
                {
                    "sym": cell.get("sym") if isinstance(cell.get("sym"), str) else " ",
                    "fg": cell.get("fg", "reset"),
                    "bg": cell.get("bg", "reset"),
                    "bold": bool(cell.get("bold", False)),
                    "reversed": bool(cell.get("reversed", False)),
                    "underline": bool(cell.get("underline", False)),
                    "width": max(0, min(cell_width, width)),
                }
            )
        normalized_rows.append(row)
    return width, height, normalized_rows


# ---------------------------------------------------------------------------
# Rendering
# ---------------------------------------------------------------------------

def render(width, height, rows, renderer, palette):
    default_fg, default_bg = palette["fg"], palette["bg"]
    image = Image.new("RGB", (width * renderer.cell_w, height * renderer.cell_h), default_bg)
    draw = ImageDraw.Draw(image)
    color_cache = {}

    def resolve(value, fallback):
        key = (value, fallback)
        if key in color_cache:
            return color_cache[key]
        try:
            parsed = parse_color(value)
        except (ValueError, TypeError) as error:
            sys.stderr.write(f"warning: {error}; using theme default\n")
            parsed = None
        result = parsed if parsed is not None else fallback
        color_cache[key] = result
        return result

    for y, row in enumerate(rows):
        skip = 0
        for x, cell in enumerate(row):
            if skip > 0:
                skip -= 1
                continue
            cell_width = cell["width"] if cell["width"] > 1 else 1
            fg = resolve(cell["fg"], default_fg)
            bg = resolve(cell["bg"], default_bg)
            renderer.draw_cell(
                draw, x, y, cell_width, cell["sym"], fg, bg,
                bold=cell["bold"],
                reversed_=cell["reversed"],
                underline=cell["underline"],
            )
            skip = cell_width - 1
    return image


def default_output_path(width, height, screen, theme, snapshot_path):
    script = Path(__file__).resolve()
    tui_dir = script.parents[1] if len(script.parents) > 1 else Path.cwd()
    stem = Path(snapshot_path).stem if snapshot_path and snapshot_path != "-" else "snapshot"
    name = f"{screen or stem}-{width}x{height}-{theme or 'unknown'}.png"
    return tui_dir / "target" / "screenshots" / name


def build_arg_parser():
    parser = argparse.ArgumentParser(
        description="Render a TypeWords TUI shot JSON snapshot to a PNG.",
        epilog=(
            "example: render-snapshot.py %TEMP%\\home.json  "
            "-> tui/target/screenshots/home-120x40-dark.png"
        ),
    )
    parser.add_argument("input", help="snapshot JSON path, or '-' for stdin")
    parser.add_argument("output", nargs="?", default=None, help="output PNG path")
    parser.add_argument("--out", dest="out", default=None, help="output PNG path")
    parser.add_argument("--scale", type=int, default=2, help="pixel scale factor (default 2)")
    parser.add_argument("--font", dest="font", default=None,
                        help="monospace font path (default: auto-detect)")
    parser.add_argument("--cjk-font", dest="cjk_font", default=None,
                        help="CJK font path (default: auto-detect)")
    parser.add_argument("--font-size", type=int, default=None,
                        help="override font pixel size")
    parser.add_argument("--cell-width", type=int, default=None,
                        help="override cell width in pixels")
    parser.add_argument("--cell-height", type=int, default=None,
                        help="override cell height in pixels")
    parser.add_argument("--screen", default=None,
                        help="screen name used for the default output file name")
    parser.add_argument("--theme", default=None,
                        help="theme name used for default colours and output file name")
    parser.add_argument("--list-fonts", action="store_true",
                        help="print the auto-detected font paths and exit")
    return parser


def main(argv=None):
    args = build_arg_parser().parse_args(argv)
    mono_path, cjk_path = resolve_fonts(args.font, args.cjk_font)
    if args.list_fonts:
        print(f"mono: {mono_path}")
        print(f"cjk:  {cjk_path}")
        return 0

    data, source = load_snapshot(args.input)
    width, height, rows = normalize_snapshot(data)
    screen = args.screen or data.get("screen") or Path(source).stem
    theme = args.theme or data.get("theme") or "dark"

    scale = max(1, int(args.scale))
    renderer = GlyphRenderer(
        mono_path,
        cjk_path,
        scale,
        font_size=args.font_size,
        cell_width=args.cell_width,
        cell_height=args.cell_height,
    )
    palette = theme_palette(theme)
    image = render(width, height, rows, renderer, palette)

    output = args.out or args.output
    output_path = Path(output) if output else default_output_path(width, height, screen, theme, args.input)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    image.save(output_path)

    print(
        f"rendered {width}x{height} cells (screen={screen}, theme={theme}, scale={scale})"
        f" -> {output_path} [{image.width}x{image.height}px]"
    )
    print(
        f"  fonts: mono={mono_path.name} ({renderer.mono.size}px), "
        f"cjk={cjk_path.name} ({renderer.cjk.size}px), "
        f"cell={renderer.cell_w}x{renderer.cell_h}px"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
