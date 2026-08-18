#!/usr/bin/env python3
"""QUINTE pixel she-wolf (lupa) — black and gold, Roman Republic symbol"""
import struct, zlib

W = H = 64
PAL = {
    '.': (0, 0, 0, 0),
    'K': (11, 10, 8, 255),
    'G': (212, 175, 55, 255),
    'L': (242, 223, 167, 255),
    'D': (122, 100, 32, 255),
    'E': (26, 21, 18, 255),
    'W': (236, 229, 211, 255),
}
img = [['.' for _ in range(W)] for _ in range(H)]

def px(x, y, c):
    if 0 <= x < W and 0 <= y < H:
        img[y][x] = c

def rect(x0, y0, x1, y1, c):
    for y in range(y0, y1 + 1):
        for x in range(x0, x1 + 1):
            px(x, y, c)

def line(x0, y0, x1, y1, c):
    dx, dy = abs(x1 - x0), abs(y1 - y0)
    sx = 1 if x0 < x1 else -1
    sy = 1 if y0 < y1 else -1
    err = dx - dy
    while True:
        px(x0, y0, c)
        if x0 == x1 and y0 == y1:
            break
        e2 = 2 * err
        if e2 > -dy: err -= dy; x0 += sx
        if e2 < dx: err += dx; y0 += sy

rect(0, 0, W - 1, H - 1, 'K')

# ---------- she-wolf (facing left, alert stance) ----------
def tri(x0, y0, x1, y1, x2, y2, c):
    # scanline-filled triangle
    pts = sorted([(x0, y0), (x1, y1), (x2, y2)], key=lambda p: p[1])
    (ax, ay), (bx, by), (cx2, cy2) = pts
    for y in range(ay, cy2 + 1):
        if by != ay and y <= by:
            xa = ax + (bx - ax) * (y - ay) / (by - ay)
        else:
            xa = bx + (cx2 - bx) * (y - by) / max(1, cy2 - by)
        if cy2 != ay:
            xb = ax + (cx2 - ax) * (y - ay) / (cy2 - ay)
        else:
            xb = xa
        for x in range(int(min(xa, xb)), int(max(xa, xb)) + 1):
            px(x, y, c)

# ears (solid triangles)
tri(13, 8, 10, 15, 17, 15, 'G'); tri(13, 9, 12, 14, 15, 14, 'D')
tri(20, 9, 17, 15, 23, 15, 'G'); tri(20, 10, 19, 14, 21, 14, 'D')
# skull
rect(11, 14, 24, 23, 'G')
line(12, 14, 22, 14, 'L')          # crown highlight
px(23, 15, 'D')
# muzzle (tapering) + nose
rect(7, 18, 11, 22, 'G')
rect(5, 19, 7, 21, 'G')
px(4, 20, 'L'); px(4, 21, 'E')
# jaw + cheek fur
line(7, 23, 14, 24, 'D')
for x in range(12, 17, 2):
    line(x, 24, x - 1, 27, 'G')
# eye
px(16, 18, 'E'); px(16, 17, 'L')
# neck (forward-leaning, thick)
rect(22, 19, 31, 40, 'G')
line(29, 20, 29, 26, 'D')          # mane line behind the neck
px(30, 22, 'D'); px(30, 25, 'D')
# deep chest
rect(23, 32, 28, 43, 'G')
px(24, 42, 'D'); px(25, 43, 'D')
# back → waist
rect(29, 23, 48, 33, 'G')
line(29, 23, 47, 23, 'L')
rect(31, 33, 43, 35, 'G')
rect(33, 35, 41, 36, 'D')          # tucked belly
px(34, 36, 'D'); px(38, 36, 'D')
# round haunch
rect(46, 25, 52, 39, 'G')
rect(48, 37, 52, 40, 'D')
# front legs
rect(25, 43, 28, 55, 'G')
rect(29, 44, 31, 55, 'D')
rect(24, 55, 29, 57, 'G')
rect(28, 55, 32, 57, 'G')
# hind legs (thigh forward, lower leg vertical)
rect(40, 39, 45, 47, 'G')
rect(42, 39, 44, 47, 'D')
rect(40, 47, 43, 55, 'G')
rect(46, 40, 49, 47, 'D')
rect(45, 47, 48, 55, 'G')
rect(39, 55, 44, 57, 'G')
rect(44, 55, 49, 57, 'G')
# tail (bushy, drooping then sweeping back)
line(51, 29, 56, 34, 'G'); line(52, 29, 57, 35, 'G'); line(52, 30, 58, 36, 'D')
line(56, 34, 58, 42, 'G'); line(57, 35, 58, 42, 'D')
line(58, 42, 56, 47, 'G'); line(57, 42, 55, 46, 'D')
px(55, 47, 'L'); px(56, 48, 'L')
# ground gold line
rect(14, 59, 54, 59, 'D')
rect(22, 59, 48, 59, 'G')

def write_png(path, pixels, scale):
    w, h = W * scale, H * scale
    raw = bytearray()
    for y in range(H):
        for _ in range(scale):
            raw.append(0)
            for x in range(W):
                r, g, b, a = PAL[pixels[y][x]]
                for _ in range(scale):
                    raw += bytes((r, g, b, a))
    def chunk(tag, data):
        c = tag + data
        return struct.pack('>I', len(data)) + c + struct.pack('>I', zlib.crc32(c))
    png = (b'\x89PNG\r\n\x1a\n'
           + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 6, 0, 0, 0))
           + chunk(b'IDAT', zlib.compress(bytes(raw), 9))
           + chunk(b'IEND', b''))
    open(path, 'wb').write(png)

write_png('/tmp/quinte-wolf.png', img, 4)
write_png('/tmp/quinte-wolf-1024.png', img, 16)
print('ok')
