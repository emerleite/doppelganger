import { describe, it, expect, beforeEach } from "vitest";
import {
  humanBytes,
  splitPath,
  looksLikeCopy,
  loadState,
  saveState,
  STATE_KEY,
  computeTotals,
  type ClusterLike,
} from "./utils";

// ---------- humanBytes ----------
describe("humanBytes", () => {
  it("formats zero", () => expect(humanBytes(0)).toBe("0 B"));
  it("formats bytes (no decimals)", () => expect(humanBytes(512)).toBe("512 B"));
  it("formats kilobytes (one decimal)", () => expect(humanBytes(1024)).toBe("1.0 KB"));
  it("formats megabytes", () => expect(humanBytes(1024 * 1024)).toBe("1.0 MB"));
  it("formats gigabytes", () =>
    expect(humanBytes(1.5 * 1024 ** 3)).toBe("1.5 GB"));
  it("formats terabytes", () =>
    expect(humanBytes(2 * 1024 ** 4)).toBe("2.0 TB"));
  it("caps at TB even for huge values", () =>
    expect(humanBytes(1e18)).toMatch(/TB$/));
  it("handles boundary 1023 B as bytes", () =>
    expect(humanBytes(1023)).toBe("1023 B"));
  it("handles boundary 1024 B as 1.0 KB", () =>
    expect(humanBytes(1024)).toBe("1.0 KB"));
  it("returns 0 B for negative input", () =>
    expect(humanBytes(-1)).toBe("0 B"));
  it("returns 0 B for NaN", () =>
    expect(humanBytes(Number.NaN)).toBe("0 B"));
  it("returns 0 B for Infinity", () =>
    expect(humanBytes(Number.POSITIVE_INFINITY)).toBe("0 B"));
});

// ---------- splitPath ----------
describe("splitPath", () => {
  it("splits absolute path", () =>
    expect(splitPath("/a/b/c.jpg")).toEqual({ dir: "/a/b", name: "c.jpg" }));
  it("splits root-level file", () =>
    expect(splitPath("/file.jpg")).toEqual({ dir: "", name: "file.jpg" }));
  it("returns name-only when no slash", () =>
    expect(splitPath("photo.jpg")).toEqual({ dir: "", name: "photo.jpg" }));
  it("handles trailing slash as empty name", () =>
    expect(splitPath("/dir/")).toEqual({ dir: "/dir", name: "" }));
  it("handles paths with spaces", () =>
    expect(splitPath("/Users/me/Pictures/IMG 1.heic")).toEqual({
      dir: "/Users/me/Pictures",
      name: "IMG 1.heic",
    }));
});

// ---------- looksLikeCopy (mirror of Rust heuristic) ----------
describe("looksLikeCopy", () => {
  it.each([
    "foo (1)",
    "foo (12)",
    "foo (123)",
    "foo_(1)",
    "foo-(1)",
    "foo copy",
    "foo_copy",
    "foo-copy",
    "foo copy 2",
    "foo Copy",
    "foo COPY",
  ])("flags %s as a copy", (s) => expect(looksLikeCopy(s)).toBe(true));

  it.each([
    "foo",
    "IMG_2670",
    "DSC01234",
    "2024-06-17_12-34-56",
    "the copy of file",
    "file(1)", // no separator before paren
    "file (abc)", // letters inside paren
    "file ()", // empty paren
    "",
  ])("does not flag %s as a copy", (s) => expect(looksLikeCopy(s)).toBe(false));
});

// ---------- persistence ----------
describe("loadState / saveState", () => {
  let storage: Storage;
  beforeEach(() => {
    const map = new Map<string, string>();
    storage = {
      getItem: (k) => map.get(k) ?? null,
      setItem: (k, v) => void map.set(k, v),
      removeItem: (k) => void map.delete(k),
      clear: () => map.clear(),
      key: (i) => Array.from(map.keys())[i] ?? null,
      get length() { return map.size; },
    };
  });

  it("returns empty object when nothing stored", () => {
    expect(loadState(storage)).toEqual({});
  });

  it("round-trips a state object", () => {
    const original = { folder: "/x", threshold: 8, checked: ["/x/a.jpg", "/x/b.jpg"] };
    expect(saveState(storage, original)).toBe(true);
    expect(loadState(storage)).toEqual(original);
  });

  it("returns empty object when stored value is malformed JSON", () => {
    storage.setItem(STATE_KEY, "{not valid json");
    expect(loadState(storage)).toEqual({});
  });

  it("survives storage that throws on setItem", () => {
    const broken: Storage = {
      ...storage,
      setItem: () => { throw new Error("quota"); },
    };
    expect(saveState(broken, { folder: "/x" })).toBe(false);
  });

  it("uses the key STATE_KEY", () => {
    saveState(storage, { folder: "/x" });
    expect(storage.getItem(STATE_KEY)).toContain("/x");
  });
});

// ---------- computeTotals ----------
describe("computeTotals", () => {
  const c = (sizes: number[], reclaim: number): ClusterLike => ({
    photos: sizes.map((size) => ({ size })),
    reclaimable_bytes: reclaim,
  });

  it("sums across multiple clusters", () => {
    const totals = computeTotals([
      c([100, 100], 100),
      c([50, 50, 50], 100),
    ]);
    expect(totals).toEqual({
      totalPhotos: 5,
      totalSize: 100 + 100 + 50 + 50 + 50,
      totalReclaim: 200,
    });
  });

  it("returns zeros for empty input", () => {
    expect(computeTotals([])).toEqual({
      totalPhotos: 0,
      totalSize: 0,
      totalReclaim: 0,
    });
  });

  it("counts photos correctly when reclaim is independent", () => {
    const totals = computeTotals([c([1, 2, 3], 999)]);
    expect(totals.totalPhotos).toBe(3);
    expect(totals.totalSize).toBe(6);
    expect(totals.totalReclaim).toBe(999);
  });
});
