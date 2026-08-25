import { describe, expect, it } from "vitest";
import { b64ToBytes, bytesToB64, fmtSize, joinPath, parentOf } from "./format";

describe("fmtSize", () => {
  it("formats bytes through terabytes", () => {
    expect(fmtSize(0)).toBe("0 B");
    expect(fmtSize(1023)).toBe("1023 B");
    expect(fmtSize(1024)).toBe("1.0 KB");
    expect(fmtSize(1536)).toBe("1.5 KB");
    expect(fmtSize(5 * 1024 * 1024)).toBe("5.0 MB");
    expect(fmtSize(200 * 1024 * 1024)).toBe("200 MB");
    expect(fmtSize(3 * 1024 ** 4)).toBe("3.0 TB");
  });
});

describe("parentOf", () => {
  it("walks up unix paths", () => {
    expect(parentOf("/")).toBe("/");
    expect(parentOf("/home")).toBe("/");
    expect(parentOf("/home/user")).toBe("/home");
    expect(parentOf("/home/user/")).toBe("/home");
    expect(parentOf("relative")).toBe("/");
  });
});

describe("joinPath", () => {
  it("joins without duplicate slashes", () => {
    expect(joinPath("/", "etc")).toBe("/etc");
    expect(joinPath("/home/", "user")).toBe("/home/user");
    expect(joinPath("/home", "user")).toBe("/home/user");
  });
});

describe("base64 round trip", () => {
  it("preserves arbitrary bytes", () => {
    const bytes = new Uint8Array([0, 1, 2, 127, 128, 255, 27, 91, 65]);
    expect(b64ToBytes(bytesToB64(bytes))).toEqual(bytes);
  });
  it("survives terminal escape sequences", () => {
    const seq = new TextEncoder().encode("\x1b[31mred\x1b[0m\r\n");
    expect(new TextDecoder().decode(b64ToBytes(bytesToB64(seq)))).toBe(
      "\x1b[31mred\x1b[0m\r\n",
    );
  });
});
