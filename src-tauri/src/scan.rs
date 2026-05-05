use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use czkawka_core::common::model::{CheckingMethod, HashType};
use czkawka_core::common::progress_data::ProgressData;
use czkawka_core::common::tool_data::CommonData;
use czkawka_core::common::traits::Search;
use czkawka_core::re_exported::{FilterType, HashAlg};
use czkawka_core::tools::duplicate::{DuplicateFinder, DuplicateFinderParameters};
use czkawka_core::tools::similar_images::{SimilarImages, SimilarImagesParameters};
use serde::Serialize;

use crate::keeper::pick_keeper;
use crate::{Cluster, ClusterKind, Photo, ScanResult};

const MIN_FILE_SIZE: u64 = 51_200; // 50 KiB — matches CLI default
const MIN_PREHASH_SIZE: u64 = 257_144;
const HASH_SIZE: u8 = 8;
const MAX_DIFFERENCE_DEFAULT: u32 = 6;

#[derive(Serialize, Clone, Copy)]
pub struct ProgressEvent {
    pub phase: &'static str,    // "exact" | "similar"
    pub stage_idx: u8,
    pub stage_max: u8,
    pub current: usize,
    pub total: usize,
}

fn to_photo(path: PathBuf, size: u64, mtime: u64, width: u32, height: u32) -> Photo {
    Photo {
        path: path.to_string_lossy().into_owned(),
        size,
        mtime,
        width,
        height,
    }
}

fn make_cluster(kind: ClusterKind, id: usize, photos: Vec<Photo>) -> Cluster {
    let keeper_index = pick_keeper(&photos);
    let mut sizes: Vec<u64> = photos.iter().map(|p| p.size).collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    let reclaimable_bytes: u64 = sizes.iter().skip(1).sum();
    Cluster {
        id: format!("{}-{id}", match kind { ClusterKind::Exact => "exact", ClusterKind::Similar => "similar" }),
        kind,
        photos,
        keeper_index,
        reclaimable_bytes,
    }
}

pub fn run_scan(
    root: &Path,
    max_difference: u32,
    forward: impl Fn(ProgressEvent) + Send + Sync + 'static + Clone,
) -> Result<ScanResult, String> {
    let stop_flag = Arc::new(AtomicBool::new(false));

    // ---------- Pass 1: exact duplicates ----------
    let exact_clusters = run_dup_finder(root, &stop_flag, forward.clone())?;

    // ---------- Pass 2: visually similar ----------
    let similar_clusters = run_similar_images(root, max_difference, &stop_flag, forward)?;

    Ok(ScanResult {
        exact: exact_clusters,
        similar: similar_clusters,
    })
}

fn run_dup_finder(
    root: &Path,
    stop_flag: &Arc<AtomicBool>,
    forward: impl Fn(ProgressEvent) + Send + 'static,
) -> Result<Vec<Cluster>, String> {
    let params = DuplicateFinderParameters::new(
        CheckingMethod::Hash,
        HashType::Blake3,
        true,                       // use prehash cache
        MIN_PREHASH_SIZE,           // minimal cache file size
        MIN_PREHASH_SIZE,           // minimal prehash cache file size
        false,                      // case-sensitive name comparison (irrelevant for HASH)
    );
    let mut finder = DuplicateFinder::new(params);
    finder.set_included_paths(vec![root.to_path_buf()]);
    finder.set_excluded_items(default_excludes());
    finder.set_minimal_file_size(MIN_FILE_SIZE);
    finder.set_allowed_extensions(image_extensions());

    let (tx, rx) = crossbeam_channel::unbounded::<ProgressData>();
    let forwarder = std::thread::spawn(move || {
        for ev in rx.iter() {
            forward(ProgressEvent {
                phase: "exact",
                stage_idx: ev.current_stage_idx,
                stage_max: ev.max_stage_idx,
                current: ev.entries_checked,
                total: ev.entries_to_check,
            });
        }
    });

    finder.search(stop_flag, Some(&tx));
    drop(tx);
    let _ = forwarder.join();

    let mut clusters = Vec::new();
    let mut id = 0usize;
    for groups in finder.get_files_sorted_by_hash().values() {
        for group in groups {
            let photos: Vec<Photo> = group
                .iter()
                .map(|de| to_photo(de.path.clone(), de.size, de.modified_date, 0, 0))
                .collect();
            if photos.len() < 2 { continue; }
            id += 1;
            clusters.push(make_cluster(ClusterKind::Exact, id, photos));
        }
    }
    // Sort biggest-reclaimable first
    clusters.sort_by(|a, b| b.reclaimable_bytes.cmp(&a.reclaimable_bytes));
    Ok(clusters)
}

fn run_similar_images(
    root: &Path,
    max_difference: u32,
    stop_flag: &Arc<AtomicBool>,
    forward: impl Fn(ProgressEvent) + Send + 'static,
) -> Result<Vec<Cluster>, String> {
    let max_diff = if max_difference == 0 { MAX_DIFFERENCE_DEFAULT } else { max_difference };
    let params = SimilarImagesParameters::new(
        max_diff,
        HASH_SIZE,
        HashAlg::Gradient,
        FilterType::Lanczos3,
        false, // exclude_images_with_same_size
        false, // exclude_images_with_same_resolution
    );
    let mut finder = SimilarImages::new(params);
    finder.set_included_paths(vec![root.to_path_buf()]);
    finder.set_excluded_items(default_excludes());
    finder.set_minimal_file_size(MIN_FILE_SIZE);

    let (tx, rx) = crossbeam_channel::unbounded::<ProgressData>();
    let forwarder = std::thread::spawn(move || {
        for ev in rx.iter() {
            forward(ProgressEvent {
                phase: "similar",
                stage_idx: ev.current_stage_idx,
                stage_max: ev.max_stage_idx,
                current: ev.entries_checked,
                total: ev.entries_to_check,
            });
        }
    });

    finder.search(stop_flag, Some(&tx));
    drop(tx);
    let _ = forwarder.join();

    let mut clusters = Vec::new();
    for (i, group) in finder.get_similar_images().iter().enumerate() {
        let photos: Vec<Photo> = group
            .iter()
            .map(|ie| to_photo(ie.path.clone(), ie.size, ie.modified_date, ie.width, ie.height))
            .collect();
        if photos.len() < 2 { continue; }
        clusters.push(make_cluster(ClusterKind::Similar, i + 1, photos));
    }
    clusters.sort_by(|a, b| b.reclaimable_bytes.cmp(&a.reclaimable_bytes));
    Ok(clusters)
}

fn default_excludes() -> Vec<String> {
    vec![
        "*/Photos Library.photoslibrary/*".into(),
        "*/Photo Booth Library/*".into(),
        "*/.Trash/*".into(),
        "*/Library/*".into(),
        "*/node_modules/*".into(),
    ]
}

fn image_extensions() -> Vec<String> {
    ["jpg","jpeg","png","heic","heif","tif","tiff","webp","gif","bmp"]
        .iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(path: &str, size: u64, mtime: u64) -> Photo {
        Photo { path: path.into(), size, mtime, width: 100, height: 100 }
    }

    #[test]
    fn make_cluster_id_format() {
        let c = make_cluster(ClusterKind::Exact, 3, vec![photo("/a", 1, 0), photo("/b", 1, 0)]);
        assert_eq!(c.id, "exact-3");
        let c = make_cluster(ClusterKind::Similar, 7, vec![photo("/a", 1, 0), photo("/b", 1, 0)]);
        assert_eq!(c.id, "similar-7");
    }

    #[test]
    fn reclaimable_excludes_largest_file() {
        // sizes: 100, 50, 30, 10 → reclaimable = 50 + 30 + 10 = 90 (largest 100 kept)
        let c = make_cluster(
            ClusterKind::Exact, 1,
            vec![
                photo("/a", 100, 1),
                photo("/b", 50, 2),
                photo("/c", 30, 3),
                photo("/d", 10, 4),
            ],
        );
        assert_eq!(c.reclaimable_bytes, 90);
    }

    #[test]
    fn reclaimable_two_equal_sizes_picks_one_to_keep() {
        // two files of equal size → reclaim one of them
        let c = make_cluster(
            ClusterKind::Exact, 1,
            vec![photo("/a", 100, 1), photo("/b", 100, 2)],
        );
        assert_eq!(c.reclaimable_bytes, 100);
    }

    #[test]
    fn reclaimable_zero_for_single_photo() {
        let c = make_cluster(ClusterKind::Exact, 1, vec![photo("/a", 100, 1)]);
        assert_eq!(c.reclaimable_bytes, 0);
    }

    #[test]
    fn keeper_index_is_set_via_pick_keeper() {
        // identical resolution + size; older mtime should win (index 1)
        let c = make_cluster(
            ClusterKind::Exact, 1,
            vec![photo("/a.jpg", 10, 200), photo("/b.jpg", 10, 100)],
        );
        assert_eq!(c.keeper_index, 1);
    }

    #[test]
    fn default_excludes_contains_critical_macos_paths() {
        let excludes = default_excludes();
        assert!(excludes.iter().any(|s| s.contains("Photos Library.photoslibrary")));
        assert!(excludes.iter().any(|s| s.contains(".Trash")));
        assert!(excludes.iter().any(|s| s.contains("Photo Booth Library")));
    }

    #[test]
    fn image_extensions_includes_common_formats() {
        let exts = image_extensions();
        for needed in ["jpg", "jpeg", "png", "heic", "heif", "tiff", "webp"] {
            assert!(exts.contains(&needed.to_string()), "missing extension {needed}");
        }
    }

    #[test]
    fn image_extensions_are_lowercase() {
        for ext in image_extensions() {
            assert_eq!(ext, ext.to_lowercase(), "extension {ext} should be lowercase");
        }
    }
}
