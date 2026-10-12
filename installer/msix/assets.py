"""Draws the package's logos (installer/msix/Assets) from Mosaïk's mark, at every size Windows
asks for, sharp at each (drawn large, then reduced). Run it again after changing the mark:

    python3 installer/msix/assets.py      (needs Pillow)

The mark is the same as apps/desktop/src-tauri/icons/mosaik.svg: four rounded tiles on a 32-unit
square.
"""

from pathlib import Path

from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent / "Assets"
# (x, y, width, height, colour) on a 32-unit square, corner radius 3 units.
TILES = [
    (2, 2, 16, 16, "#2B44D9"),
    (21, 2, 9, 9, "#F4B13A"),
    (21, 14, 9, 16, "#12786A"),
    (2, 21, 16, 9, "#C93450"),
]
SUPERSAMPLE = 8


def mark(size: int) -> Image.Image:
    """The mark filling a `size` square (its own margin included), on transparency."""
    big = size * SUPERSAMPLE
    unit = big / 32
    image = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    for x, y, w, h, colour in TILES:
        box = (x * unit, y * unit, (x + w) * unit - 1, (y + h) * unit - 1)
        draw.rounded_rectangle(box, radius=3 * unit, fill=colour)
    return image.resize((size, size), Image.LANCZOS)


def canvas(width: int, height: int, art: int) -> Image.Image:
    """A transparent `width` x `height` image with the mark, `art` pixels square, centred."""
    image = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    image.alpha_composite(mark(art), ((width - art) // 2, (height - art) // 2))
    return image


def main() -> None:
    HERE.mkdir(exist_ok=True)
    out = {}
    # App icon (Start, taskbar, title bars): the mark edge to edge, as the guidelines ask for.
    for scale in (100, 125, 150, 200, 400):
        size = 44 * scale // 100
        out[f"Square44x44Logo.scale-{scale}.png"] = mark(size)
    for target in (16, 24, 32, 48, 256):
        out[f"Square44x44Logo.targetsize-{target}.png"] = mark(target)
        out[f"Square44x44Logo.targetsize-{target}_altform-unplated.png"] = mark(target)
    # Tiles: the mark at half the tile's height, centred.
    for scale in (100, 200):
        f = scale // 100
        out[f"Square150x150Logo.scale-{scale}.png"] = canvas(150 * f, 150 * f, 72 * f)
        out[f"Wide310x150Logo.scale-{scale}.png"] = canvas(310 * f, 150 * f, 72 * f)
        out[f"StoreLogo.scale-{scale}.png"] = mark(50 * f)
    for name, image in out.items():
        image.save(HERE / name, optimize=True)
    print(f"{len(out)} images in {HERE}")


if __name__ == "__main__":
    main()
