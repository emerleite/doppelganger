import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  humanBytes,
  splitPath,
  loadState,
  saveState as persistState,
  computeTotals,
  type PersistedState,
} from "./utils";

interface Photo {
  path: string;
  size: number;
  mtime: number;
  width: number;
  height: number;
}
interface Cluster {
  id: string;
  kind: "exact" | "similar";
  photos: Photo[];
  keeper_index: number;
  reclaimable_bytes: number;
}
interface ScanResult {
  exact: Cluster[];
  similar: Cluster[];
}
interface ProgressEvent {
  phase: "exact" | "similar";
  stage_idx: number;
  stage_max: number;
  current: number;
  total: number;
}

const $ = <T extends HTMLElement = HTMLElement>(id: string) =>
  document.getElementById(id) as T;

const pickBtn = $<HTMLButtonElement>("pick-folder");
const folderLabel = $("folder-label");
const thresholdInput = $<HTMLInputElement>("threshold");
const thresholdVal = $("threshold-val");
const scanBtn = $<HTMLButtonElement>("scan");
const progressBox = $("progress");
const progressFill = $("progress-fill");
const progressLabel = $("progress-label");
const results = $("results");
const footer = $("footer");
const tally = $("tally");
const trashBtn = $<HTMLButtonElement>("trash-btn");

let selectedFolder: string | null = null;
let lastResult: ScanResult | null = null;
const checkedPaths = new Set<string>();
const pathToSize = new Map<string, number>();

// ---------- Persistence (localStorage, survives app close on Tauri) ----------
function saveState() {
  const s: PersistedState = {
    folder: selectedFolder ?? undefined,
    threshold: parseInt(thresholdInput.value, 10),
    result: lastResult ?? undefined,
    checked: [...checkedPaths],
  };
  persistState(localStorage, s);
}

// Tiny DOM-builder helper. Children may be strings (auto text-noded) or nodes.
type Child = Node | string | null | undefined | false;
function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Record<string, unknown> = {},
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v == null || v === false) continue;
    if (k === "class") node.className = String(v);
    else if (k === "dataset" && typeof v === "object") Object.assign(node.dataset, v);
    else if (k.startsWith("on") && typeof v === "function")
      node.addEventListener(k.slice(2).toLowerCase(), v as EventListener);
    else node.setAttribute(k, String(v));
  }
  for (const c of children) {
    if (c == null || c === false) continue;
    node.append(typeof c === "string" ? document.createTextNode(c) : c);
  }
  return node;
}
function clear(n: HTMLElement) { while (n.firstChild) n.removeChild(n.firstChild); }

thresholdInput.addEventListener("input", () => {
  thresholdVal.textContent = thresholdInput.value;
  saveState();
});

pickBtn.addEventListener("click", async () => {
  const picked = await open({ directory: true, multiple: false });
  if (typeof picked === "string") {
    selectedFolder = picked;
    folderLabel.textContent = picked;
    folderLabel.classList.remove("muted");
    scanBtn.disabled = false;
    saveState();
  }
});

listen<ProgressEvent>("scan-progress", (e) => {
  const { phase, current, total, stage_idx, stage_max } = e.payload;
  const pct = total > 0 ? Math.round((current / total) * 100) : 0;
  progressFill.style.width = pct + "%";
  progressLabel.textContent =
    `${phase === "exact" ? "Hashing for exact duplicates" : "Comparing for visual similarity"} ` +
    `· stage ${stage_idx + 1}/${stage_max + 1} · ${current.toLocaleString()}/${total.toLocaleString()}`;
});

scanBtn.addEventListener("click", async () => {
  if (!selectedFolder) return;
  scanBtn.disabled = true;
  pickBtn.disabled = true;
  progressBox.classList.remove("hidden");
  progressFill.style.width = "0%";
  progressLabel.textContent = "Starting…";
  clear(results);
  results.append(el("p", { class: "empty" }, "Scanning…"));

  try {
    const r = await invoke<ScanResult>("scan_directory", {
      path: selectedFolder,
      maxDifference: parseInt(thresholdInput.value, 10),
    });
    lastResult = r;
    renderResults(r);
    saveState();
  } catch (err) {
    clear(results);
    results.append(el("p", { class: "empty" }, "Scan failed: " + String(err)));
  } finally {
    progressBox.classList.add("hidden");
    scanBtn.disabled = false;
    pickBtn.disabled = false;
  }
});

function renderResults(r: ScanResult, checkedOverride?: Set<string>) {
  checkedPaths.clear();
  pathToSize.clear();
  clear(results);

  if (r.exact.length === 0 && r.similar.length === 0) {
    results.append(el("p", { class: "empty" }, "No duplicate or similar photos found."));
    footer.classList.add("hidden");
    return;
  }

  // Pre-populate pathToSize so subsequent renderPhoto can decide checked state.
  for (const c of [...r.exact, ...r.similar]) {
    for (const p of c.photos) pathToSize.set(p.path, p.size);
  }
  // If we have a saved checked-set, use it. Otherwise default to "non-keepers checked".
  if (checkedOverride) {
    for (const path of checkedOverride) {
      if (pathToSize.has(path)) checkedPaths.add(path);
    }
  } else {
    for (const c of [...r.exact, ...r.similar]) {
      for (let i = 0; i < c.photos.length; i++) {
        if (i !== c.keeper_index) checkedPaths.add(c.photos[i].path);
      }
    }
  }

  results.append(renderSummary(r));
  if (r.exact.length > 0) results.append(renderSection("Exact duplicates", r.exact));
  if (r.similar.length > 0) results.append(renderSection("Visually similar", r.similar));

  footer.classList.remove("hidden");
  updateTally();
  loadThumbs();
}

function renderSummary(r: ScanResult): HTMLElement {
  const allClusters = [...r.exact, ...r.similar];
  const { totalPhotos, totalSize, totalReclaim } = computeTotals(allClusters);

  return el("div", { class: "summary" },
    el("div", { class: "summary-stat" },
      el("span", { class: "summary-label" }, "Found"),
      el("span", { class: "summary-value" },
        `${totalPhotos.toLocaleString()} photos in ${allClusters.length.toLocaleString()} clusters`),
    ),
    el("div", { class: "summary-stat" },
      el("span", { class: "summary-label" }, "Total size"),
      el("span", { class: "summary-value" }, humanBytes(totalSize)),
    ),
    el("div", { class: "summary-stat highlight" },
      el("span", { class: "summary-label" }, "Reclaimable"),
      el("span", { class: "summary-value" }, `~${humanBytes(totalReclaim)}`),
    ),
  );
}

function renderSection(title: string, clusters: Cluster[]): HTMLElement {
  const section = el("section", { class: "kind" },
    el("h2", {}, `${title} — ${clusters.length} clusters`));
  for (const c of clusters) section.append(renderCluster(c));
  return section;
}

function renderCluster(c: Cluster): HTMLElement {
  const totalSize = c.photos.reduce((a, p) => a + p.size, 0);
  const photosGrid = el("div", { class: "photos" });
  for (let i = 0; i < c.photos.length; i++) {
    photosGrid.append(renderPhoto(c.photos[i], i === c.keeper_index));
  }
  return el("div", { class: "cluster" },
    el("div", { class: "cluster-header" },
      el("span", {}, `${c.photos.length} files · ${humanBytes(totalSize)} total`),
      el("span", { class: "cluster-reclaim" }, `~${humanBytes(c.reclaimable_bytes)} reclaimable`),
    ),
    photosGrid,
  );
}

function renderPhoto(p: Photo, isKeeper: boolean): HTMLElement {
  const dirAndName = splitPath(p.path);
  const dim = p.width > 0 ? `${p.width}×${p.height}` : "—";
  const isChecked = checkedPaths.has(p.path);

  const checkbox = el("input", { type: "checkbox" }) as HTMLInputElement;
  checkbox.checked = isChecked;
  checkbox.addEventListener("change", () => {
    if (checkbox.checked) checkedPaths.add(p.path);
    else checkedPaths.delete(p.path);
    tile.classList.toggle("delete", checkbox.checked);
    tile.classList.toggle("keeper", isKeeper && !checkbox.checked);
    updateTally();
    saveState();
  });

  const thumbWrap = el("div", { class: "thumb-wrap" }, el("span", { class: "thumb-loading" }, "…"));
  const cls = "photo " + (isChecked ? "delete" : (isKeeper ? "keeper" : ""));
  const tile = el("div",
    { class: cls, dataset: { path: p.path } },
    isKeeper ? el("span", { class: "keeper-tag" }, "KEEP") : null,
    thumbWrap,
    el("div", { class: "meta" },
      el("strong", {}, dirAndName.name),
      el("br"),
      `${dim} · ${humanBytes(p.size)}`,
      el("br"),
      dirAndName.dir,
    ),
    el("label", {}, checkbox, " Move to Trash"),
  );
  return tile;
}

async function loadThumbs() {
  const tiles = Array.from(results.querySelectorAll<HTMLElement>(".photo"));
  const queue = [...tiles];

  async function worker() {
    while (queue.length) {
      const tile = queue.shift()!;
      const path = tile.dataset.path!;
      const wrap = tile.querySelector<HTMLElement>(".thumb-wrap")!;
      try {
        const dataUrl = await invoke<string>("get_thumbnail", { path });
        clear(wrap);
        const img = el("img", { alt: "", loading: "lazy" }) as HTMLImageElement;
        img.src = dataUrl;
        wrap.append(img);
      } catch {
        clear(wrap);
        wrap.append(el("span", { class: "thumb-loading" }, "no preview"));
      }
    }
  }

  await Promise.all(Array.from({ length: 4 }, () => worker()));
}

function updateTally() {
  let bytes = 0;
  checkedPaths.forEach((p) => (bytes += pathToSize.get(p) || 0));
  if (checkedPaths.size === 0) {
    tally.textContent = "No files marked.";
    trashBtn.disabled = true;
    trashBtn.textContent = "Move to Trash";
  } else {
    tally.textContent = `${checkedPaths.size} files · ${humanBytes(bytes)} marked`;
    trashBtn.disabled = false;
    trashBtn.textContent = `Move ${checkedPaths.size} to Trash`;
  }
}

trashBtn.addEventListener("click", async () => {
  if (checkedPaths.size === 0) return;
  const paths = [...checkedPaths];
  trashBtn.disabled = true;
  trashBtn.textContent = "Moving…";
  try {
    const res = await invoke<{ path: string; ok: boolean; error?: string }[]>(
      "move_to_trash",
      { paths },
    );
    let moved = 0;
    const movedSet = new Set<string>();
    for (const r of res) {
      if (r.ok) {
        moved++;
        movedSet.add(r.path);
        for (const tile of results.querySelectorAll<HTMLElement>(".photo")) {
          if (tile.dataset.path === r.path) tile.remove();
        }
        checkedPaths.delete(r.path);
      } else {
        console.error("Trash failed", r);
      }
    }
    // Drop the trashed entries from lastResult so they don't come back on reload.
    if (lastResult && movedSet.size > 0) {
      const purge = (clusters: Cluster[]) =>
        clusters
          .map((c) => ({
            ...c,
            photos: c.photos.filter((p) => !movedSet.has(p.path)),
          }))
          .filter((c) => c.photos.length >= 2);
      lastResult = {
        exact: purge(lastResult.exact),
        similar: purge(lastResult.similar),
      };
    }
    saveState();
    alert(`Moved ${moved} of ${paths.length} files to Trash. Recover via Finder → Put Back.`);
  } catch (err) {
    alert(`Trash failed: ${err}`);
  } finally {
    updateTally();
  }
});

// ---------- Restore state on app start ----------
(function restore() {
  const s = loadState(localStorage);
  if (typeof s.threshold === "number") {
    thresholdInput.value = String(s.threshold);
    thresholdVal.textContent = thresholdInput.value;
  }
  if (s.folder) {
    selectedFolder = s.folder;
    folderLabel.textContent = s.folder;
    folderLabel.classList.remove("muted");
    scanBtn.disabled = false;
  }
  const result = s.result as ScanResult | undefined;
  if (result && (result.exact.length > 0 || result.similar.length > 0)) {
    lastResult = result;
    const checked = s.checked ? new Set(s.checked) : undefined;
    renderResults(result, checked);
  }
})();

