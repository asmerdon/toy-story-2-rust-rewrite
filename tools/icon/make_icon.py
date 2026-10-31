"""Makes the OpenTS2 icon from the game's own Pizza Planet token.

The token is Level 1's special object 0x39 (mesh Shape395, texture tex16). Export it from
your install first, then run this:

    cargo run --release --example token_model -- "<game dir>" out/token
    python tools/icon/make_icon.py out/token

It draws the token flat, face-on (its texture and silver rim as they are, no shading), then
rotates that 2D picture to the tilt the token has in game. Writes to assets/icon/:
opents2.ico (16-256 px), opents2.png (256), opents2-1024.png and the installer's wizard
pictures (wizard-small*.bmp, wizard-large*.bmp). Needs Pillow.

The icon is the one piece of game art the project ships, by the owner's decision
(PROJECT.md, ground rules).
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path(__file__).resolve().parents[2] / "assets" / "icon"
# Anticlockwise tilt of the flat picture, in degrees: the ring runs from lower left to upper
# right, as the token is seen in game.
TILT = 48


def load(src):
    verts, uvs, faces, mat = [], [], [], None
    for line in (src / "token.obj").read_text().splitlines():
        p = line.split()
        if not p:
            continue
        if p[0] == "v":
            verts.append(tuple(map(float, p[1:7])))
        elif p[0] == "vt":
            uvs.append(tuple(map(float, p[1:3])))
        elif p[0] == "usemtl":
            mat = p[1]
        elif p[0] == "f":
            faces.append((mat, [int(x.split("/")[0]) - 1 for x in p[1:4]]))
    tex = {m: Image.open(src / f"{m}.bmp").convert("RGB") for m, _ in faces if m and (src / f"{m}.bmp").exists()}
    return verts, uvs, faces, tex


def render_flat(verts, uvs, faces, tex, size):
    """The token face-on: x right, y up, nearest faces drawn last; texels kept sharp."""
    centre = [(min(v[k] for v in verts) + max(v[k] for v in verts)) / 2 for k in range(3)]
    radius = max(abs(v[k] - centre[k]) for v in verts for k in (0, 1))
    s = size * 0.48 / radius
    scr = [(size / 2 + (v[0] - centre[0]) * s, size / 2 - (v[1] - centre[1]) * s) for v in verts]
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    for mat, (a, b, c) in sorted(faces, key=lambda f: -sum(verts[i][2] for i in f[1])):
        tri = [scr[a], scr[b], scr[c]]
        x0 = int(min(t[0] for t in tri)) - 1
        y0 = int(min(t[1] for t in tri)) - 1
        bw = int(max(t[0] for t in tri)) - x0 + 2
        bh = int(max(t[1] for t in tri)) - y0 + 2
        local = [(t[0] - x0, t[1] - y0) for t in tri]
        mask = Image.new("L", (bw, bh), 0)
        ImageDraw.Draw(mask).polygon(local, fill=255)
        t = tex.get(mat)
        if t is None:
            # The rim: its vertex colour (bytes 1-3, 0x80 = 1.0), as the game lights it.
            col = tuple(min(255, int(255 * sum(verts[i][3 + k] for i in (a, b, c)) / 3)) for k in range(3))
            patch = Image.new("RGB", (bw, bh), col)
        else:
            tw, th = t.size
            uv = [(uvs[i][0] * tw, (1 - uvs[i][1]) * th) for i in (a, b, c)]
            (sx0, sy0), (sx1, sy1), (sx2, sy2) = local
            det = (sx1 - sx0) * (sy2 - sy0) - (sx2 - sx0) * (sy1 - sy0)
            if abs(det) < 1e-9:
                continue

            def affine(k):
                q0, q1, q2 = uv[0][k], uv[1][k], uv[2][k]
                ca = ((q1 - q0) * (sy2 - sy0) - (q2 - q0) * (sy1 - sy0)) / det
                cb = ((q2 - q0) * (sx1 - sx0) - (q1 - q0) * (sx2 - sx0)) / det
                return ca, cb, q0 - ca * sx0 - cb * sy0

            patch = t.transform((bw, bh), Image.AFFINE, affine(0) + affine(1), resample=Image.NEAREST)
        img.paste(patch.convert("RGBA"), (x0, y0), mask)
    return img


def square(img, margin=1.04):
    """Crops to the picture and centres it in a square with a small margin."""
    img = img.crop(img.getbbox())
    side = int(max(img.size) * margin)
    sq = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    sq.alpha_composite(img, ((side - img.width) // 2, (side - img.height) // 2))
    return sq


def on_white(img, w, h, scale):
    bg = Image.new("RGBA", (w, h), (255, 255, 255, 255))
    n = int(min(w, h) * scale)
    bg.alpha_composite(img.resize((n, n), Image.LANCZOS), ((w - n) // 2, (h - n) // 2))
    return bg.convert("RGB")


def main():
    src = Path(sys.argv[1] if len(sys.argv) > 1 else "out/token")
    flat = render_flat(*load(src), 2048)
    # Rotate the 2D picture. At this size each texel is a block of ~30 pixels, so the
    # rotation keeps the pixel-art look with clean edges.
    tilted = flat.rotate(TILT, resample=Image.BICUBIC, expand=True)
    big = square(tilted).resize((1024, 1024), Image.LANCZOS)
    OUT.mkdir(parents=True, exist_ok=True)
    big.save(OUT / "opents2-1024.png")
    big.resize((256, 256), Image.LANCZOS).save(OUT / "opents2.png")
    big.save(OUT / "opents2.ico", sizes=[(n, n) for n in (16, 24, 32, 48, 64, 128, 256)])
    # Inno Setup's wizard pictures (100% and 200% DPI).
    on_white(big, 55, 55, 1.0).save(OUT / "wizard-small.bmp")
    on_white(big, 110, 110, 1.0).save(OUT / "wizard-small-200.bmp")
    on_white(big, 164, 314, 0.9).save(OUT / "wizard-large.bmp")
    on_white(big, 328, 628, 0.9).save(OUT / "wizard-large-200.bmp")
    print("wrote", OUT)


if __name__ == "__main__":
    main()
