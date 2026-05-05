# Doppelganger

> Find and trash duplicate or visually-similar photos. Native desktop app — macOS, Windows, Linux. Built with Tauri + Rust.

Doppelganger finds **exact duplicates** and **visually-similar near-duplicates** in your photo library and helps you reclaim disk space — without sending anything to the cloud. It uses perceptual hashing (the same approach behind tools like CCleaner) so it catches the same photo across resizes, recompressions, and minor edits, not just byte-identical copies. Built with [Tauri 2](https://tauri.app) and the [czkawka](https://github.com/qarmin/czkawka) Rust core, so the whole app is one small native binary on macOS, Windows, and Linux. Selected files are moved to the system Trash — fully reversible via your file manager's "Put Back."

## Features

- **Two scan passes** — exact-duplicate detection (blake3) and visually-similar detection (perceptual hashing).
- **HEIC supported** out of the box — works with modern iPhone photo libraries.
- **Tunable similarity** — slider for how aggressive the near-dup detection should be.
- **Smart "keeper" suggestion** — for each cluster, the app picks the photo most likely to be the original (highest resolution → not a `(1)`/`copy` filename → oldest mtime → largest file).
- **Reversible** — files go to the system Trash, never `rm`. Recover via Finder/Explorer "Put Back."
- **Stays local** — your photos never leave your machine.
- **State persisted** — close and reopen the app and you land back on your last scan, with the same checkboxes ticked.

## Quick start (development)

Prerequisites: [Rust](https://rustup.rs/) (1.92+), [Bun](https://bun.sh/) (or Node 20+ / pnpm / npm), and platform-specific webview deps for [Tauri](https://tauri.app/start/prerequisites/).

```bash
git clone <this-repo> doppelganger
cd doppelganger
bun install
bun run tauri dev
```

## Building a release locally

```bash
bun run tauri build
```

Artifacts land in `src-tauri/target/release/bundle/`.

## CI / release pipeline

Pushing a `v*` tag (or running the **Release** workflow manually from the Actions tab) triggers `.github/workflows/release.yml`, which builds for:

- macOS (Apple Silicon + Intel)
- Linux (x64)
- Windows (x64)

…and creates a draft GitHub Release with all artifacts attached. Code-signing slots for macOS are scaffolded — add the relevant secrets to the repo and uncomment the `env:` block to ship signed builds.

## Tests

```bash
# Rust unit tests (keeper heuristic + scan helpers)
cd src-tauri && cargo test --lib

# Frontend unit tests
bun run test

# Mutation testing (~3 min, requires `cargo install cargo-mutants`)
cd src-tauri && cargo mutants --file src/keeper.rs
```

The test suite covers:

- 37 Rust tests on the keeper-selection heuristic, including discriminator tests for every branch in the copy-detection regex.
- 8 Rust tests on cluster/reclaimable-bytes math.
- 45 Vitest cases on the frontend pure helpers (formatting, persistence, state aggregation).
- **100% mutation kill rate** on `keeper.rs` (31/31 mutants caught).

## Architecture

```
┌─────────────── frontend (Vanilla TS + Vite) ───────────────┐
│  index.html   src/main.ts   src/utils.ts   src/styles.css  │
│             folder picker · scan UI · cluster grid          │
└────────────────────┬────────────────────────────────────────┘
                     │  Tauri IPC (commands + events)
┌────────────────────┴────────────────────────────────────────┐
│ src-tauri/src                                               │
│   lib.rs    Tauri commands: scan_directory · get_thumbnail  │
│             · move_to_trash                                 │
│   scan.rs   wraps czkawka_core (DuplicateFinder +           │
│             SimilarImages); forwards progress events        │
│   keeper.rs original-keeper heuristic (pure, fully tested)  │
│   thumb.rs  qlmanage shellout for HEIC thumbnails (macOS)   │
└─────────────────────────────────────────────────────────────┘
```

State lives in webview localStorage under the key `doppelganger-state-v1`. Thumbnails are cached on disk in `/tmp/doppelganger-thumbs/`.

## Limitations

- **macOS-only thumbnails right now.** The Rust thumbnail path shells out to `qlmanage`. Linux and Windows ports replace this with the `image` crate + libheif (planned).
- **No app icon yet** — the bundled Tauri starter icons are placeholders.
- **Photos.app library is excluded by design** — modifying its bundle corrupts macOS Photos.

## Credits

- Duplicate / similarity detection by [czkawka](https://github.com/qarmin/czkawka) (MIT).
- Native shell by [Tauri](https://tauri.app).
- macOS Trash flow uses the [`trash`](https://crates.io/crates/trash) crate so files retain Finder "Put Back" metadata.

## License

MIT.
