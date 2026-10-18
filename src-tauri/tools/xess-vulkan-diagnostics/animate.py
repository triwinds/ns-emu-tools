"""Create 3-3-2 palette GIF previews from this harness's RGB8 PNGs, stdlib only.

Use original PNGs for colour/quality assessment; GIF quantisation is lossy.
"""
import pathlib
import struct
import sys
import zlib


def png_rgb(path):
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    at, compressed = 8, bytearray()
    while at < len(data):
        length = struct.unpack_from(">I", data, at)[0]
        kind = data[at + 4:at + 8]
        chunk = data[at + 8:at + 8 + length]
        if kind == b"IHDR":
            width, height, depth, colour, _, _, interlace = struct.unpack(">IIBBBBB", chunk)
            assert (depth, colour, interlace) == (8, 2, 0)
        elif kind == b"IDAT":
            compressed.extend(chunk)
        at += 12 + length
    raw, stride = zlib.decompress(compressed), width * 3
    rgb = bytearray(stride * height)
    for y in range(height):
        filter_type = raw[y * (stride + 1)]
        for x in range(stride):
            left = rgb[y * stride + x - 3] if x >= 3 else 0
            up = rgb[(y - 1) * stride + x] if y else 0
            corner = rgb[(y - 1) * stride + x - 3] if y and x >= 3 else 0
            if filter_type == 4:
                p = left + up - corner
                distances = (abs(p - left), abs(p - up), abs(p - corner))
                prediction = (left, up, corner)[distances.index(min(distances))]
            else:
                prediction = (0, left, up, (left + up) // 2)[filter_type]
            rgb[y * stride + x] = (raw[y * (stride + 1) + 1 + x] + prediction) & 255
    return width, height, rgb


def gif(paths, output):
    frames = [png_rgb(p) for p in paths]
    width, height, _ = frames[0]
    data = bytearray(b"GIF89a" + struct.pack("<HHBBB", width, height, 0xF7, 0, 0))
    for i in range(256):
        data.extend((round(((i >> 5) & 7) * 255 / 7), round(((i >> 2) & 7) * 255 / 7), round((i & 3) * 255 / 3)))
    data.extend(b"!\xff\x0bNETSCAPE2.0\x03\x01\x00\x00\x00")
    for w, h, rgb in frames:
        assert (w, h) == (width, height)
        # 8 hundredths of a second per frame; deliberately slow inspection loop.
        data.extend(b"!\xf9\x04\x04\x08\x00\x00\x00")
        data.extend(b"," + struct.pack("<HHHHB", 0, 0, width, height, 0) + b"\x08")
        indices = [(rgb[i] & 0xE0) | ((rgb[i + 1] & 0xE0) >> 3) | (rgb[i + 2] >> 6) for i in range(0, len(rgb), 3)]
        packed, accumulator, bits = bytearray(), 0, 0
        # Reset before the dictionary reaches 512 entries: every code remains
        # nine bits. Simpler than compression, and ample for small crop previews.
        codes = []
        for offset in range(0, len(indices), 200):
            codes.append(256)
            codes.extend(indices[offset:offset + 200])
        codes.append(257)
        for code in codes:
            accumulator |= code << bits
            bits += 9
            while bits >= 8:
                packed.append(accumulator & 255)
                accumulator >>= 8
                bits -= 8
        if bits:
            packed.append(accumulator & 255)
        for offset in range(0, len(packed), 255):
            block = packed[offset:offset + 255]
            data.append(len(block))
            data.extend(block)
        data.append(0)
    data.append(0x3B)
    output.write_bytes(data)


if __name__ == "__main__":
    evidence = pathlib.Path(sys.argv[1])
    for mode in ("aa", "quality", "balanced"):
        paths = sorted(evidence.glob(f"{mode}-motion-*.png"))
        if paths:
            output = evidence / f"{mode}-motion.gif"
            gif(paths, output)
            print(output)
