#!/usr/bin/env python3
"""生成游戏用到的文件资源（纯标准库，无需 PIL / numpy）。

输出：
  assets/textures/ship_sheet.png   4 帧飞船精灵表 (128x32)，尾焰逐帧变化
  assets/textures/star.png         五角星 (32x32)
  assets/audio/collect.wav         收集音效（上行琶音，0.18s）
  assets/audio/music.wav           背景音乐循环（8 小节合成器 pad，~9.6s）

用法：
  python3 scripts/gen_assets.py            # 生成到默认目录
  python3 scripts/gen_assets.py --check    # 只校验文件存在且格式正确（CI 用）
"""

from __future__ import annotations

import argparse
import math
import struct
import sys
import wave
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"

# --------------------------------------------------------------------------
# PNG
# --------------------------------------------------------------------------


def write_png(path: Path, width: int, height: int, pixels: list[tuple[int, int, int, int]]) -> None:
    """pixels: 行优先 RGBA8。"""
    assert len(pixels) == width * height
    raw = bytearray()
    for y in range(height):
        raw.append(0)  # filter: None
        for x in range(width):
            raw.extend(pixels[y * width + x])

    def chunk(tag: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(png)


def clamp8(v: float) -> int:
    return max(0, min(255, int(round(v))))


def point_in_triangle(p, a, b, c) -> bool:
    def sign(p1, p2, p3):
        return (p1[0] - p3[0]) * (p2[1] - p3[1]) - (p2[0] - p3[0]) * (p1[1] - p3[1])

    d1, d2, d3 = sign(p, a, b), sign(p, b, c), sign(p, c, a)
    has_neg = d1 < 0 or d2 < 0 or d3 < 0
    has_pos = d1 > 0 or d2 > 0 or d3 > 0
    return not (has_neg and has_pos)


def ship_frame(frame: int, size: int = 32) -> list[tuple[int, int, int, int]]:
    """朝上的飞船；frame 控制尾焰长度，形成 4 帧循环动画。"""
    px: list[tuple[int, int, int, int]] = []
    flame_len = [5, 8, 6, 9][frame % 4]
    for y in range(size):
        for x in range(size):
            # 以中心为原点，y 向上
            u = x + 0.5 - size / 2
            v = size / 2 - (y + 0.5)
            col = (0, 0, 0, 0)
            # 尾焰（先画，被机身覆盖）
            if abs(u) <= 3 and -9 - flame_len <= v <= -8:
                t = (-8 - v) / max(flame_len, 1)
                col = (255, clamp8(220 - 160 * t), clamp8(80 - 80 * t), clamp8(255 - 150 * t))
            # 机翼
            if point_in_triangle((u, v), (-13, -9), (13, -9), (0, 4)):
                col = (70, 110, 200, 255)
            # 机身
            if point_in_triangle((u, v), (-6, -10), (6, -10), (0, 14)):
                shade = 170 + int(50 * (v + 10) / 24)
                col = (clamp8(shade), clamp8(shade), 255, 255)
            # 驾驶舱
            if (u * u) / 4 + ((v - 3) ** 2) / 9 <= 1:
                col = (120, 255, 240, 255)
            px.append(col)
    return px


def ship_sheet() -> tuple[int, int, list[tuple[int, int, int, int]]]:
    frames = [ship_frame(i) for i in range(4)]
    w, h = 32 * 4, 32
    out: list[tuple[int, int, int, int]] = []
    for y in range(h):
        for f in range(4):
            out.extend(frames[f][y * 32 : (y + 1) * 32])
    return w, h, out


def star_image(size: int = 32) -> list[tuple[int, int, int, int]]:
    px = []
    outer, inner = size * 0.47, size * 0.2
    for y in range(size):
        for x in range(size):
            u = x + 0.5 - size / 2
            v = size / 2 - (y + 0.5)
            r = math.hypot(u, v)
            ang = math.atan2(u, v)  # 0 朝上
            sector = math.tau / 5
            a = (ang % sector) / sector  # 0..1
            # 在 outer/inner 之间线性插值出五角星边界
            edge = inner + (outer - inner) * abs(1 - 2 * a)
            if r <= edge:
                glow = 1 - r / outer
                px.append((255, clamp8(200 + 55 * glow), clamp8(60 + 150 * glow), 255))
            else:
                px.append((0, 0, 0, 0))
    return px


# --------------------------------------------------------------------------
# WAV
# --------------------------------------------------------------------------

SR = 22_050


def write_wav(path: Path, samples: list[float]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(b"".join(struct.pack("<h", int(max(-1.0, min(1.0, s)) * 32000)) for s in samples))


def envelope(i: int, n: int, attack: float = 0.01, release: float = 0.3) -> float:
    t = i / SR
    dur = n / SR
    a = min(1.0, t / attack) if attack > 0 else 1.0
    r = min(1.0, (dur - t) / release) if release > 0 else 1.0
    return max(0.0, min(a, r))


def collect_sfx() -> list[float]:
    notes = [880.0, 1108.7, 1318.5]  # A5 C#6 E6
    out: list[float] = []
    for f in notes:
        n = int(SR * 0.06)
        for i in range(n):
            out.append(0.5 * math.sin(math.tau * f * i / SR) * envelope(i, n, 0.003, 0.04))
    return out


def music_loop() -> list[float]:
    # Am - F - C - G，每和弦 2 拍 @100bpm
    bpm = 100
    beat = 60 / bpm
    chords = [
        [220.0, 261.6, 329.6],
        [174.6, 220.0, 261.6],
        [261.6, 329.6, 392.0],
        [196.0, 246.9, 293.7],
    ] * 2
    out: list[float] = []
    for ci, chord in enumerate(chords):
        n = int(SR * beat * 2)
        for i in range(n):
            t = i / SR
            s = sum(math.sin(math.tau * f * t) + 0.3 * math.sin(math.tau * 2 * f * t) for f in chord) / (len(chord) * 1.3)
            # 琶音高音层
            arp = chord[(i // int(SR * beat / 2)) % 3] * 2
            s += 0.25 * math.sin(math.tau * arp * t) * envelope(i % int(SR * beat / 2), int(SR * beat / 2), 0.005, 0.15)
            out.append(0.28 * s * envelope(i, n, 0.05, 0.08))
    return out


# --------------------------------------------------------------------------


def png_mean_luma(path: Path) -> float:
    """纯标准库 PNG 解码（8bit RGB/RGBA，非隔行），返回平均亮度 0..255。
    用于窗口冒烟：判断截图是不是“全黑”（窗口被遮挡/显示器休眠时 macOS 不渲染）。"""
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a png"
    pos, idat, width = 8, bytearray(), 0
    height = bpp = 0
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        tag = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        if tag == b"IHDR":
            width, height, depth, ctype, _, _, interlace = struct.unpack(">IIBBBBB", body)
            assert depth == 8 and ctype in (2, 6) and interlace == 0, "只支持 8bit RGB/RGBA"
            bpp = 3 if ctype == 2 else 4
        elif tag == b"IDAT":
            idat += body
        pos += 12 + length
    raw = zlib.decompress(bytes(idat))
    stride = width * bpp
    prev = bytearray(stride)
    total, count, i = 0, 0, 0
    for _ in range(height):
        ftype = raw[i]
        line = bytearray(raw[i + 1 : i + 1 + stride])
        i += 1 + stride
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0
            b = prev[x]
            c = prev[x - bpp] if x >= bpp else 0
            if ftype == 1:
                line[x] = (line[x] + a) & 0xFF
            elif ftype == 2:
                line[x] = (line[x] + b) & 0xFF
            elif ftype == 3:
                line[x] = (line[x] + ((a + b) >> 1)) & 0xFF
            elif ftype == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                line[x] = (line[x] + pred) & 0xFF
        for x in range(0, stride, bpp * 4):  # 采样 1/4 像素足够
            total += (line[x] * 299 + line[x + 1] * 587 + line[x + 2] * 114) // 1000
            count += 1
        prev = line
    return total / max(count, 1)


def check() -> int:
    required = {
        ASSETS / "textures/ship_sheet.png": b"\x89PNG",
        ASSETS / "textures/star.png": b"\x89PNG",
        ASSETS / "audio/collect.wav": b"RIFF",
        ASSETS / "audio/music.wav": b"RIFF",
        ASSETS / "config/game.cfg.ron": None,
    }
    ok = True
    for path, magic in required.items():
        if not path.exists():
            print(f"[MISSING] {path.relative_to(ROOT)}")
            ok = False
            continue
        if magic and not path.read_bytes().startswith(magic):
            print(f"[BAD] {path.relative_to(ROOT)} 文件头不对")
            ok = False
            continue
        print(f"[OK] {path.relative_to(ROOT)} ({path.stat().st_size} bytes)")
    return 0 if ok else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--luma", metavar="PNG", help="打印 PNG 平均亮度（0..255）")
    args = ap.parse_args()
    if args.luma:
        print(f"{png_mean_luma(Path(args.luma)):.2f}")
        return 0
    if args.check:
        return check()

    w, h, px = ship_sheet()
    write_png(ASSETS / "textures/ship_sheet.png", w, h, px)
    write_png(ASSETS / "textures/star.png", 32, 32, star_image())
    write_wav(ASSETS / "audio/collect.wav", collect_sfx())
    write_wav(ASSETS / "audio/music.wav", music_loop())
    print("assets generated:")
    return check()


if __name__ == "__main__":
    sys.exit(main())
