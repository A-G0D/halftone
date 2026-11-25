#!/usr/bin/env python3
"""Scores 1-bit dithers on the same grid and times Pillow / numpy on the same job.

Needs `cargo bench --bench image` to have written bench/out/*_braille_*.pgm first.
Quality = PSNR (dB) between the 5x5 box-blurred 1-bit grid and the grey source box-filtered to
that grid (luma 0.2126/0.7152/0.0722, the same weights and box bounds halftone uses).
Speed = source Mpix/s for: box-filter to the grid + luma + 1-bit dither. Python -I, Pillow, numpy.
"""
import glob, os, sys, time
import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))

def read_pgm(p):
    b = open(p, "rb").read()
    _, dims, _, rest = b.split(b"\n", 3)
    w, h = map(int, dims.split())
    return (np.frombuffer(rest, np.uint8).reshape(h, w) > 0).astype(np.float64)

def box_grid(rgb, gw, gh):
    """Same cell bounds as halftone's `sample`: x0 = gx*w/gw, at least one source px per cell."""
    h, w, _ = rgb.shape
    def edges(n, g):
        s = np.arange(g) * n // g
        return s, np.maximum((np.arange(g) + 1) * n // g, s + 1).clip(max=n)
    ys, ye = edges(h, gh); xs, xe = edges(w, gw)
    c = np.zeros((h + 1, w + 1, 3)); c[1:, 1:] = rgb.cumsum(0).cumsum(1)
    s = c[ye][:, xe] - c[ys][:, xe] - c[ye][:, xs] + c[ys][:, xs]
    n = ((ye - ys)[:, None] * (xe - xs)[None, :])[..., None]
    return s / n / 255.0

def luma(g): return g @ np.array([0.2126, 0.7152, 0.0722])

def blur(a, k=5):
    p = np.pad(a, k // 2, mode="edge"); o = np.zeros_like(a)
    for dy in range(k):
        for dx in range(k):
            o += p[dy:dy + a.shape[0], dx:dx + a.shape[1]]
    return o / (k * k)

def psnr(bits, grey):
    mse = ((blur(bits) - blur(grey)) ** 2).mean()
    return 10 * np.log10(1 / mse)

def best_time(f, n=7):
    f(); t = []
    for _ in range(n):
        s = time.perf_counter(); f(); t.append(time.perf_counter() - s)
    return sorted(t)[n // 2]

BAYER = (np.array([0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5]).reshape(4, 4) + 0.5) / 16 - 0.5

def pil_fs(rgb, gw, gh):
    g = (luma(box_grid(rgb, gw, gh)) * 255).round().astype(np.uint8)
    return np.asarray(Image.fromarray(g, "L").convert("1")).astype(np.float64)  # Floyd-Steinberg by default

def np_bayer(rgb, gw, gh):
    g = luma(box_grid(rgb, gw, gh))
    t = np.tile(BAYER, (gh // 4 + 1, gw // 4 + 1))[:gh, :gw]
    return (g + t > 0.5).astype(np.float64)

print("image,method,psnr_db,mpix_per_s")
for pgm in sorted(glob.glob(os.path.join(HERE, "out", "*_braille_*.pgm"))):
    stem = os.path.basename(pgm)[:-4]
    name, label = stem.split("_braille_")
    bits = read_pgm(pgm); gh, gw = bits.shape
    rgb = np.asarray(Image.open(os.path.join(HERE, "images", name + ".bmp")).convert("RGB"), dtype=np.float64)
    mp = rgb.shape[0] * rgb.shape[1] / 1e6
    grey = luma(box_grid(rgb, gw, gh))
    print(f"{name},halftone braille {label},{psnr(bits, grey):.2f},")
    if label == "bayer":
        print(f"{name},numpy bayer4x4,{psnr(np_bayer(rgb, gw, gh), grey):.2f},{mp / best_time(lambda: np_bayer(rgb, gw, gh)):.1f}")
    if label == "floyd":
        print(f"{name},Pillow convert('1') floyd,{psnr(pil_fs(rgb, gw, gh), grey):.2f},{mp / best_time(lambda: pil_fs(rgb, gw, gh)):.1f}")
