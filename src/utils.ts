// Pure helpers extracted from main.ts so they're unit-testable.

export function humanBytes(n: number): string {
  if (n < 0 || !Number.isFinite(n)) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return (i === 0 ? n : n.toFixed(1)) + " " + units[i];
}

export function splitPath(p: string): { dir: string; name: string } {
  const idx = p.lastIndexOf("/");
  return idx < 0
    ? { dir: "", name: p }
    : { dir: p.slice(0, idx), name: p.slice(idx + 1) };
}

/**
 * Mirror of the Rust `looks_like_copy` heuristic, used only in tests to assert
 * the front-end and back-end stay in sync. NOT used at runtime — the Rust side
 * decides keeper_index server-side.
 */
export function looksLikeCopy(stem: string): boolean {
  const s = stem.replace(/\s+$/, "");
  const lower = s.toLowerCase();

  const idx = lower.lastIndexOf("copy");
  if (idx >= 0) {
    const before = s.slice(0, idx);
    const after = s.slice(idx + 4);
    const precededOk =
      before.length === 0 || /[ _\-]$/.test(before);
    const trailingOk = /^[\s_\-0-9]*$/.test(after);
    if (precededOk && trailingOk && idx + 4 + after.length === s.length) {
      return true;
    }
  }

  if (s.endsWith(")")) {
    const open = s.lastIndexOf("(");
    if (open >= 0) {
      const inside = s.slice(open + 1, -1);
      const before = s.slice(0, open);
      if (
        inside.length > 0 &&
        /^\d+$/.test(inside) &&
        (before.replace(/\s+$/, "").length === 0 || /[ _\-]$/.test(before))
      ) {
        return true;
      }
    }
  }
  return false;
}

export interface PersistedState {
  folder?: string;
  threshold?: number;
  result?: unknown;
  checked?: string[];
}

export const STATE_KEY = "doppelganger-state-v1";

export function loadState(storage: Storage): PersistedState {
  try {
    const raw = storage.getItem(STATE_KEY);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

export function saveState(storage: Storage, state: PersistedState): boolean {
  try {
    storage.setItem(STATE_KEY, JSON.stringify(state));
    return true;
  } catch {
    return false;
  }
}

/** Compute totals for a scan result: { totalPhotos, totalSize, totalReclaim } */
export interface ClusterLike {
  photos: { size: number }[];
  reclaimable_bytes: number;
}
export function computeTotals(clusters: ClusterLike[]): {
  totalPhotos: number;
  totalSize: number;
  totalReclaim: number;
} {
  let totalPhotos = 0;
  let totalSize = 0;
  let totalReclaim = 0;
  for (const c of clusters) {
    totalPhotos += c.photos.length;
    for (const p of c.photos) totalSize += p.size;
    totalReclaim += c.reclaimable_bytes;
  }
  return { totalPhotos, totalSize, totalReclaim };
}
