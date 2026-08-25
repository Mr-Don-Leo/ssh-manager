// Generates simple app icons (rounded-square, accent blue, ">" glyph)
// as PNGs without any image library, using zlib + hand-built pixel buffers.
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = join(root, "src-tauri", "icons");
mkdirSync(outDir, { recursive: true });

function crc32(buf) {
  let table = crc32.table;
  if (!table) {
    table = crc32.table = new Int32Array(256).map((_, n) => {
      let c = n;
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      return c;
    });
  }
  let crc = -1;
  for (const b of buf) crc = (crc >>> 8) ^ table[(crc ^ b) & 0xff];
  return (crc ^ -1) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

function png(size, pixelFn) {
  // RGBA rows, each prefixed with filter byte 0
  const raw = Buffer.alloc(size * (size * 4 + 1));
  for (let y = 0; y < size; y++) {
    const rowStart = y * (size * 4 + 1);
    raw[rowStart] = 0;
    for (let x = 0; x < size; x++) {
      const [r, g, b, a] = pixelFn(x, y, size);
      const o = rowStart + 1 + x * 4;
      raw[o] = r;
      raw[o + 1] = g;
      raw[o + 2] = b;
      raw[o + 3] = a;
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

// Rounded-square accent-blue tile with a white "❯" chevron.
function pixel(x, y, size) {
  const r = size * 0.22;
  const cx = Math.min(Math.max(x, r), size - r);
  const cy = Math.min(Math.max(y, r), size - r);
  const dist = Math.hypot(x - cx, y - cy);
  if (dist > r) return [0, 0, 0, 0];
  const edge = Math.min(1, Math.max(0, r - dist));
  // chevron: two strokes forming ">"
  const u = x / size;
  const v = y / size;
  const w = 0.075; // stroke half-width
  const inBand = (p, q) => Math.abs(p - q) < w;
  const upper = v >= 0.3 && v <= 0.52 && inBand(u - 0.34, (v - 0.3) * 0.95);
  const lower = v > 0.52 && v <= 0.74 && inBand(u - 0.34, (0.74 - v) * 0.95);
  const base = [0, 122, 255];
  const white = [255, 255, 255];
  const c = upper || lower ? white : base;
  return [c[0], c[1], c[2], Math.round(255 * edge)];
}

for (const size of [32, 128, 512]) {
  const name = size === 512 ? "icon.png" : `${size}x${size}.png`;
  writeFileSync(join(outDir, name), png(size, pixel));
  console.log("wrote", name);
}
