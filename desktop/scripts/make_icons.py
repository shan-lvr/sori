"""Generate Sori's app icon source (1024px) and the menu-bar template icon.

uv run --with pillow python scripts/make_icons.py && npx tauri icon src-tauri/icon-src.png
"""
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent / "src-tauri"
BARS = [0.28, 0.55, 0.85, 0.55, 0.28]

S = 1024
grad = Image.new("RGBA", (S, S))
gd = ImageDraw.Draw(grad)
for y in range(S):
    t = y / S
    gd.line([(0, y), (S, y)], fill=(int(92 - 40 * t), int(110 - 30 * t), int(255 - 20 * t), 255))
mask = Image.new("L", (S, S), 0)
ImageDraw.Draw(mask).rounded_rectangle([100, 100, S - 100, S - 100], radius=190, fill=255)
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
img.paste(grad, (0, 0), mask)
d = ImageDraw.Draw(img)
w, gap = 62, 46
x0 = (S - (len(BARS) * w + (len(BARS) - 1) * gap)) // 2
for i, h in enumerate(BARS):
    hh = int(440 * h)
    x, y = x0 + i * (w + gap), (S - hh) // 2
    d.rounded_rectangle([x, y, x + w, y + hh], radius=w // 2, fill=(255, 255, 255, 255))
img.save(ROOT / "icon-src.png")

T = 64
tray = Image.new("RGBA", (T, T), (0, 0, 0, 0))
td = ImageDraw.Draw(tray)
bw, bg = 7, 5
tx = (T - (5 * bw + 4 * bg)) // 2
for i, h in enumerate(BARS):
    hh = int(46 * h)
    x, y = tx + i * (bw + bg), (T - hh) // 2
    td.rounded_rectangle([x, y, x + bw, y + hh], radius=3, fill=(0, 0, 0, 255))
(ROOT / "icons").mkdir(exist_ok=True)
tray.save(ROOT / "icons" / "tray.png")
