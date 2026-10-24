"""Render unscaled PNGs and exact 2x crops from same-frame Vulkan readbacks."""
import argparse
from pathlib import Path
import struct
import zlib

def png(path, width, height, rgb):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind+data))
    scan = b"".join(b"\0"+rgb[y*width*3:(y+1)*width*3] for y in range(height))
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR",struct.pack(">IIBBBBB",width,height,8,2,0,0,0)) + chunk(b"IDAT",zlib.compress(scan)) + chunk(b"IEND",b""))

p = argparse.ArgumentParser()
p.add_argument("captures", type=Path)
p.add_argument("frame", type=int)
p.add_argument("--stage", default="xess-native")
args = p.parse_args()
images = []
for i in (0, 1):
    magic, size, maximum, data = (args.captures / f"{args.frame:06}-{args.stage}-{i}.ppm").read_bytes().split(b"\n",3)
    assert magic == b"P6" and maximum == b"255"
    width,height = map(int,size.split())
    assert len(data) == width*height*3
    images.append(data)
for im, label in zip(images, ("off", args.stage)):
    png(args.captures / f"{args.frame:06}-{label}.png",width,height,im)
regions = [(1050, 700, 1450, 1000), (1270, 90, 1670, 390), (300, 700, 700, 1000)]
sheet = bytearray([24] * (1600*1830*3))
for row, region in enumerate(regions):
    for col, im in enumerate(images):
        x,y,right,bottom = region
        for dy in range(bottom-y):
            source = im[((y+dy)*width+x)*3:((y+dy)*width+right)*3]
            expanded = b"".join(source[k:k+3]*2 for k in range(0,len(source),3))
            for repeat in (0,1):
                at = ((row*610+dy*2+repeat)*1600+col*800)*3
                sheet[at:at+len(expanded)] = expanded
png(args.captures / f"{args.frame:06}-same-frame-crops.png",1600,1830,sheet)
