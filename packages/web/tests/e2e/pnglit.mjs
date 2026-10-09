// A minimal PNG reader for the e2e suites' pixel checks (Node's zlib does
// the deflate; nothing here needs an npm package). Supports what Chromium
// screenshots produce: 8-bit RGB/RGBA, non-interlaced.
import zlib from "node:zlib";

/// Decode a PNG into { width, height, lit: [{x, y, b}] } where a pixel is
/// lit when its brightness passes `threshold` (default 60 of 255 — the
/// graph background is #0c0e13).
export function litPixels(png, threshold = 60) {
  if (png.readUInt32BE(0) !== 0x89504e47) throw new Error("not a PNG");
  let off = 8;
  let width = 0, height = 0, colorType = 6, bitDepth = 8, interlace = 0;
  const idat = [];
  while (off < png.length) {
    const len = png.readUInt32BE(off);
    const type = png.toString("ascii", off + 4, off + 8);
    const data = png.subarray(off + 8, off + 8 + len);
    if (type === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      bitDepth = data[8];
      colorType = data[9];
      interlace = data[12];
    } else if (type === "IDAT") {
      idat.push(data);
    } else if (type === "IEND") break;
    off += 12 + len;
  }
  if (bitDepth !== 8 || interlace !== 0 || (colorType !== 6 && colorType !== 2)) {
    throw new Error(`unsupported PNG: depth ${bitDepth}, color ${colorType}, interlace ${interlace}`);
  }
  const bpp = colorType === 6 ? 4 : 3;
  const raw = zlib.inflateSync(Buffer.concat(idat));
  const stride = width * bpp;
  const out = Buffer.alloc(stride * height);
  let p = 0;
  for (let y = 0; y < height; y++) {
    const filter = raw[p++];
    const line = raw.subarray(p, p + stride);
    p += stride;
    const prev = y > 0 ? out.subarray((y - 1) * stride, y * stride) : null;
    const cur = out.subarray(y * stride, (y + 1) * stride);
    for (let i = 0; i < stride; i++) {
      const a = i >= bpp ? cur[i - bpp] : 0;
      const b = prev ? prev[i] : 0;
      const c = i >= bpp && prev ? prev[i - bpp] : 0;
      let v = line[i];
      if (filter === 1) v = (v + a) & 0xff;
      else if (filter === 2) v = (v + b) & 0xff;
      else if (filter === 3) v = (v + ((a + b) >> 1)) & 0xff;
      else if (filter === 4) {
        const pa = Math.abs(b - c), pb = Math.abs(a - c), pc = Math.abs(a + b - 2 * c);
        v = (v + (pa <= pb && pa <= pc ? a : pb <= pc ? b : c)) & 0xff;
      }
      cur[i] = v;
    }
  }
  const lit = [];
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const i = y * stride + x * bpp;
      const b = (out[i] + out[i + 1] + out[i + 2]) / 3;
      if (b > threshold) lit.push({ x, y, b });
    }
  }
  return { width, height, lit };
}
