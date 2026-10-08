#!/usr/bin/env python3
"""eat_hwp_addon 툴바 아이콘(toolbar.bmp) 생성기.

HWP 툴바 비트맵 포맷(hello_hwp_dll 샘플과 동일):
  - 960×16, 32bit, BI_RGB(무압축), bits offset 54, bottom-up, BGRA
  - 알파는 사용하지 않음(전 픽셀 255). 배경은 회색 RGB(220,220,220).
  - 0번 셀(좌측 16×16)만 image_index 0으로 사용.

아이콘: 파란 문서(텍스트 줄 3개) + 우측 금색 4-점 반짝이(=AI) → "AI 문서 작성".
수정하려면 GRID/팔레트를 고치고 다시 실행:  python3 tools/make_icon.py
"""
import struct
from pathlib import Path

W, H, CELL = 960, 16, 16
BG = (220, 220, 220)

# 16×16 픽셀 맵 (y=0 위쪽). 각 줄 정확히 16자.
GRID = [
    "................",
    "..oooooooo......",
    "..o######o......",
    "..o######o......",
    "..o#----#o......",
    "..o######o......",
    "..o#----#o......",
    "..o######o...+..",
    "..o#----#o...+..",
    "..o######o.++*++",
    "..o######o...+..",
    "..o######o...+..",
    "..oooooooo......",
    "................",
    "................",
    "................",
]
PALETTE = {
    ".": BG,              # 배경(회색)
    "o": (30, 90, 170),   # 문서 외곽선(진한 파랑)
    "#": (255, 255, 255), # 문서 면(흰색)
    "-": (120, 170, 230), # 텍스트 줄(연한 파랑)
    "+": (255, 170, 0),   # 반짝이 팔(주황)
    "*": (255, 210, 70),  # 반짝이 중심(금색)
}

assert all(len(row) == CELL for row in GRID) and len(GRID) == CELL, "GRID는 16×16 이어야 함"

# 960×16 캔버스를 배경으로 채우고 0번 셀에 아이콘을 그린다. [y][x] = (r,g,b)
canvas = [[BG for _ in range(W)] for _ in range(H)]
for y, row in enumerate(GRID):
    for x, ch in enumerate(row):
        canvas[y][x] = PALETTE[ch]

# BMP 직렬화 (bottom-up, BGRA, alpha=255)
pixels = bytearray()
for y in reversed(range(H)):          # 파일 첫 줄 = 이미지 맨 아래줄
    for x in range(W):
        r, g, b = canvas[y][x]
        pixels += bytes((b, g, r, 255))

img_size = W * H * 4
file_header = b"BM" + struct.pack("<IHHI", 54 + img_size, 0, 0, 54)
info_header = struct.pack(
    "<IiiHHIIiiII", 40, W, H, 1, 32, 0, img_size, 3779, 3779, 0, 0
)

out = Path(__file__).resolve().parent.parent / "toolbar.bmp"
out.write_bytes(file_header + info_header + bytes(pixels))
print(f"wrote {out} ({out.stat().st_size} bytes)")
