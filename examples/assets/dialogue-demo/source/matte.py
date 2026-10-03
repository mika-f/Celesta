"""Recovers a transparent PNG from the same image drawn on black and on white.

usage: matte.py BLACK.png WHITE.png OUT.png

Where a pixel is opaque both renders agree; where it is transparent they
differ by the full black-to-white range. So alpha = 1 - (white - black), and
the straight color is the black render divided by alpha.
"""
import sys

from PIL import Image

black = Image.open(sys.argv[1]).convert('RGB')
white = Image.open(sys.argv[2]).convert('RGB')
out = Image.new('RGBA', black.size)
pixels = []
for b, w in zip(black.getdata(), white.getdata()):
    alpha = 255 - max(0, min(255, round(sum(wc - bc for bc, wc in zip(b, w)) / 3)))
    if alpha == 0:
        pixels.append((0, 0, 0, 0))
    else:
        pixels.append(tuple(min(255, round(c * 255 / alpha)) for c in b) + (alpha,))
out.putdata(pixels)
out.save(sys.argv[3], optimize=True)
