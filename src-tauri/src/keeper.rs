use std::path::Path;

use crate::Photo;

/// Detect filenames that look like OS/browser-generated duplicates:
///   "foo (1)", "foo (12)", "foo copy", "foo copy 2", "foo Copy"
/// Camera-style names like "IMG_2670" are NOT flagged.
fn looks_like_copy(stem: &str) -> bool {
    let s = stem.trim_end();
    let lower = s.to_ascii_lowercase();

    // " copy" / "_copy" / "-copy" optionally followed by space + digits
    if let Some(idx) = lower.rfind("copy") {
        let before = &s[..idx];
        let after = &s[idx + 4..];
        let preceded_ok = before.is_empty() || before.ends_with([' ', '_', '-']);
        let trailing_ok = after
            .trim()
            .chars()
            .all(|c| c.is_ascii_digit() || c == ' ' || c == '_' || c == '-');
        if preceded_ok && trailing_ok && idx + 4 == s.len() - after.len() {
            return true;
        }
    }

    // "(<digits>)" at the very end, preceded by space/underscore/dash
    if s.ends_with(')') {
        if let Some(open) = s.rfind('(') {
            let inside = &s[open + 1..s.len() - 1];
            let before = &s[..open];
            if !inside.is_empty()
                && inside.chars().all(|c| c.is_ascii_digit())
                && (before.trim_end().is_empty()
                    || before.ends_with([' ', '_', '-']))
            {
                return true;
            }
        }
    }
    false
}

pub fn pick_keeper(photos: &[Photo]) -> usize {
    if photos.is_empty() {
        return 0;
    }
    photos
        .iter()
        .enumerate()
        .max_by_key(|(_, p)| {
            let stem = Path::new(&p.path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let pixels = (p.width as u64) * (p.height as u64);
            let original_looking = !looks_like_copy(stem);
            // max_by_key picks the largest:
            //   pixels                  — larger wins
            //   original_looking (bool) — true beats false (original beats copy)
            //   Reverse(mtime)          — older wins
            //   size                    — larger wins (tiebreaker)
            (pixels, original_looking, std::cmp::Reverse(p.mtime), p.size)
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(path: &str, w: u32, h: u32, size: u64, mtime: u64) -> Photo {
        Photo {
            path: path.to_string(),
            width: w,
            height: h,
            size,
            mtime,
        }
    }

    // ---------- looks_like_copy: positive cases ----------
    #[test] fn copy_paren_one()        { assert!(looks_like_copy("foo (1)")); }
    #[test] fn copy_paren_two_digit()  { assert!(looks_like_copy("foo (12)")); }
    #[test] fn copy_paren_three_digit(){ assert!(looks_like_copy("foo (123)")); }
    #[test] fn copy_paren_underscore() { assert!(looks_like_copy("foo_(1)")); }
    #[test] fn copy_paren_dash()       { assert!(looks_like_copy("foo-(1)")); }
    #[test] fn copy_word_lowercase()   { assert!(looks_like_copy("foo copy")); }
    #[test] fn copy_word_underscore()  { assert!(looks_like_copy("foo_copy")); }
    #[test] fn copy_word_dash()        { assert!(looks_like_copy("foo-copy")); }
    #[test] fn copy_word_with_number() { assert!(looks_like_copy("foo copy 2")); }
    #[test] fn copy_word_capitalized() { assert!(looks_like_copy("foo Copy")); }
    #[test] fn copy_word_uppercase()   { assert!(looks_like_copy("foo COPY")); }

    // Mutation-test discriminators for the `.all()` predicate:
    // each of `digit / space / underscore / dash` must be reachable in `after.trim()`.
    #[test] fn copy_with_space_in_trail()      { assert!(looks_like_copy("foo copy 2 3")); }
    #[test] fn copy_with_underscore_in_trail() { assert!(looks_like_copy("foo_copy_2")); }
    #[test] fn copy_with_dash_in_trail()       { assert!(looks_like_copy("foo-copy-2")); }

    // ---------- looks_like_copy: negative cases ----------
    #[test] fn plain_filename_not_copy()    { assert!(!looks_like_copy("foo")); }
    #[test] fn camera_name_not_copy()       { assert!(!looks_like_copy("IMG_2670")); }
    #[test] fn dsc_camera_name_not_copy()   { assert!(!looks_like_copy("DSC01234")); }
    #[test] fn date_stamped_not_copy()      { assert!(!looks_like_copy("2024-06-17_12-34-56")); }
    #[test] fn copy_in_middle_not_copy()    { assert!(!looks_like_copy("the copy of file")); }
    #[test] fn paren_no_space_not_copy()    { assert!(!looks_like_copy("file(1)")); }
    #[test] fn paren_letters_not_copy()     { assert!(!looks_like_copy("file (abc)")); }
    #[test] fn empty_paren_not_copy()       { assert!(!looks_like_copy("file ()")); }
    #[test] fn empty_string_not_copy()      { assert!(!looks_like_copy("")); }

    // ---------- pick_keeper: priority rules ----------
    #[test]
    fn highest_resolution_wins_outright() {
        let photos = vec![
            p("/x/a.jpg", 100, 100, 10, 100),
            p("/x/b.jpg", 200, 200, 10, 100),
            p("/x/c.jpg", 50, 50, 10, 100),
        ];
        assert_eq!(pick_keeper(&photos), 1);
    }

    #[test]
    fn larger_resolution_beats_original_filename() {
        let photos = vec![
            p("/x/foo.jpg", 100, 100, 10, 100),
            p("/x/foo (1).jpg", 200, 200, 10, 200),
        ];
        assert_eq!(pick_keeper(&photos), 1);
    }

    #[test]
    fn original_wins_over_copy_at_same_resolution() {
        let photos = vec![
            p("/x/quiz (1).gif", 100, 100, 10, 200),
            p("/x/quiz.gif", 100, 100, 10, 100),
        ];
        assert_eq!(pick_keeper(&photos), 1);
    }

    #[test]
    fn original_wins_when_resolution_unknown() {
        // both have width=0 (czkawka leaves these zero for exact-dup mode)
        let photos = vec![
            p("/x/quiz (1).gif", 0, 0, 1024, 200),
            p("/x/quiz.gif", 0, 0, 1024, 100),
        ];
        assert_eq!(pick_keeper(&photos), 1);
    }

    #[test]
    fn older_mtime_wins_when_neither_is_copy() {
        let photos = vec![
            p("/x/a.jpg", 100, 100, 10, 200),
            p("/x/b.jpg", 100, 100, 10, 100),
        ];
        assert_eq!(pick_keeper(&photos), 1);
    }

    #[test]
    fn larger_size_breaks_final_tie() {
        // identical pixels, both originals, same mtime → larger size wins
        let photos = vec![
            p("/x/a.jpg", 100, 100, 10, 100),
            p("/x/b.jpg", 100, 100, 99, 100),
            p("/x/c.jpg", 100, 100, 50, 100),
        ];
        assert_eq!(pick_keeper(&photos), 1);
    }

    #[test]
    fn copy_loses_even_when_larger_size() {
        // copy with bigger file but same pixels & mtime should still lose to original
        let photos = vec![
            p("/x/foo.jpg", 100, 100, 10, 100),
            p("/x/foo (1).jpg", 100, 100, 99999, 100),
        ];
        assert_eq!(pick_keeper(&photos), 0);
    }

    #[test]
    fn empty_input_returns_zero() {
        assert_eq!(pick_keeper(&[]), 0);
    }

    #[test]
    fn single_photo_is_keeper() {
        let photos = vec![p("/x/lonely.jpg", 100, 100, 10, 100)];
        assert_eq!(pick_keeper(&photos), 0);
    }

    #[test]
    fn three_way_tie_breaks_deterministically() {
        // pixels equal, all "original-looking", mtime equal, size equal → first wins (max_by_key
        // returns last element on ties; std::cmp tuple comparison is lexicographic; we expect a
        // stable, predictable index). Document the actual behavior so refactors notice.
        let photos = vec![
            p("/x/a.jpg", 100, 100, 10, 100),
            p("/x/b.jpg", 100, 100, 10, 100),
            p("/x/c.jpg", 100, 100, 10, 100),
        ];
        // max_by_key returns the *last* maximum on full ties.
        assert_eq!(pick_keeper(&photos), 2);
    }

    #[test]
    fn keeper_prefers_known_dimensions_over_unknown() {
        // mtime/size identical, one has dims, one doesn't → dims wins
        let photos = vec![
            p("/x/a.jpg", 0, 0, 10, 100),
            p("/x/b.jpg", 100, 100, 10, 100),
        ];
        assert_eq!(pick_keeper(&photos), 1);
    }

    #[test]
    fn handles_path_with_no_extension() {
        let photos = vec![
            p("/x/file (1)", 100, 100, 10, 200),
            p("/x/file", 100, 100, 10, 100),
        ];
        assert_eq!(pick_keeper(&photos), 1);
    }

    #[test]
    fn handles_path_with_dots_in_name() {
        let photos = vec![
            p("/x/a.b.c (1).jpg", 100, 100, 10, 200),
            p("/x/a.b.c.jpg", 100, 100, 10, 100),
        ];
        assert_eq!(pick_keeper(&photos), 1);
    }

    #[test]
    fn pixels_uses_product_not_sum() {
        // Mutation discriminator: w*h vs w+h.
        // a: 10×10 = 100 (sum 20), mtime newer
        // b:  9×11 =  99 (sum 20), mtime older
        // With * (correct):  a wins on pixels (100 > 99)
        // With + (mutant):   tied on sum (20 == 20), then older mtime → b wins
        let photos = vec![
            p("/x/a.jpg", 10, 10, 0, 200),
            p("/x/b.jpg",  9, 11, 0, 100),
        ];
        assert_eq!(pick_keeper(&photos), 0);
    }
}
