use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use os_str_bytes::OsStrBytesExt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// A filesystem entry returned by [`walk_entries`].
#[cfg_attr(test, derive(Debug))]
pub enum Entry {
    File(PathBuf),
    Dir(PathBuf),
}

pub fn is_glob(path: &Path) -> bool {
    ['*', '?', '[', '{']
        .iter()
        .any(|ch| OsStrBytesExt::contains(path.as_os_str(), *ch))
}

/// Returns the longest leading prefix of `pattern` that contains no glob characters.
/// Returns `"."` when the pattern starts with a glob (e.g. `"**/*.rs"`).
pub fn base_dir_from_pattern(pattern: &Path) -> PathBuf {
    let mut base = PathBuf::new();
    for component in pattern.components() {
        if is_glob(component.as_os_str().as_ref()) {
            break;
        }
        base.push(component);
    }
    if base.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        base
    }
}

/// Builds a `GlobSet` that matches the full path of files matching `pattern`.
///
/// Uses `literal_separator(true)` so that `*` stays within a single directory
/// level (standard shell behaviour). Use `**` to match across directories.
pub fn build_matcher(pattern: &str) -> Result<GlobSet, globset::Error> {
    let glob = GlobBuilder::new(pattern)
        .case_insensitive(true)
        .literal_separator(true)
        .build()?;
    GlobSetBuilder::new().add(glob).build()
}

/// Builds a `GlobSet` that a directory path must match before recursing into it.
///
/// Returns an empty set (matches nothing) when `pattern` has no directory component,
/// which prevents descending into any subdirectory for flat patterns like `"foo*"`.
pub fn build_dir_matcher(pattern: &Path) -> GlobSet {
    let empty = GlobSetBuilder::new()
        .build()
        .expect("empty GlobSet always builds");

    let parent = match pattern.parent() {
        None => return empty,
        Some(p) => p,
    };

    if parent.as_os_str().is_empty() || parent == Path::new(".") {
        return empty;
    }

    let mut builder = GlobSetBuilder::new();
    let mut prefix = PathBuf::new();
    for component in parent.components() {
        prefix.push(component);
        if let Some(s) = prefix.to_str()
            && let Ok(glob) = GlobBuilder::new(s)
                .literal_separator(true)
                .case_insensitive(true)
                .build()
        {
            builder.add(glob);
        }
    }
    builder.build().unwrap_or(empty)
}

/// Walks the filesystem and returns all files matching a path-pattern (one that
/// contains a `/`). The walk root is the longest literal prefix of `pattern`.
pub fn walk(pattern: &Path) -> io::Result<Vec<PathBuf>> {
    let pat_str = pattern.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "glob pattern must be valid UTF-8",
        )
    })?;
    let matcher =
        build_matcher(pat_str).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let dir_matcher = build_dir_matcher(pattern);
    let base = base_dir_from_pattern(pattern);
    let mut files = vec![];
    walk_recursive(&base, &matcher, &dir_matcher, &mut files)?;
    Ok(files)
}

/// Walks the filesystem and returns all files **and directories** matching
/// `pattern`. Matching directories are returned as [`Entry::Dir`] so the
/// caller can decide how to expand them (e.g. with `**/*.au.toml`). When a
/// directory matches the pattern it is yielded but not descended into —
/// contents are left to the caller.
pub fn walk_entries(pattern: &Path) -> io::Result<Vec<Entry>> {
    let pat_str = pattern.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "glob pattern must be valid UTF-8",
        )
    })?;
    let matcher =
        build_matcher(pat_str).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let dir_matcher = build_dir_matcher(pattern);
    let base = base_dir_from_pattern(pattern);
    let mut entries = vec![];
    walk_recursive_entries(&base, &matcher, &dir_matcher, &mut entries)?;
    Ok(entries)
}

fn walk_recursive_entries(
    dir: &Path,
    matcher: &GlobSet,
    dir_matcher: &GlobSet,
    entries: &mut Vec<Entry>,
) -> io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for item in fs::read_dir(dir)? {
        let path = item?.path();
        if path.is_dir() {
            if matcher.is_match(&path) {
                // Directory matched — yield it; caller expands its contents.
                entries.push(Entry::Dir(path));
            } else if dir_matcher.is_match(&path) {
                walk_recursive_entries(&path, matcher, dir_matcher, entries)?;
            }
        } else if path.is_file() && matcher.is_match(&path) {
            entries.push(Entry::File(path));
        }
    }
    Ok(())
}

fn walk_recursive(
    dir: &Path,
    matcher: &GlobSet,
    dir_matcher: &GlobSet,
    files: &mut Vec<PathBuf>,
) -> io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            if dir_matcher.is_match(&path) {
                walk_recursive(&path, matcher, dir_matcher, files)?;
            }
        } else if path.is_file() && matcher.is_match(&path) {
            files.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- is_glob ---

    #[test]
    fn is_glob_star() {
        assert!(is_glob(Path::new("*.rs")));
    }

    #[test]
    fn is_glob_double_star() {
        assert!(is_glob(Path::new("**/*.rs")));
    }

    #[test]
    fn is_glob_question_mark() {
        assert!(is_glob(Path::new("foo?.rs")));
    }

    #[test]
    fn is_glob_bracket() {
        assert!(is_glob(Path::new("foo[12].rs")));
    }

    #[test]
    fn is_glob_brace() {
        assert!(is_glob(Path::new("foo{a,b}.rs")));
    }

    #[test]
    fn is_glob_literal_path() {
        assert!(!is_glob(Path::new("src/foo.rs")));
    }

    // --- base_dir_from_pattern ---

    #[test]
    fn base_dir_double_star() {
        assert_eq!(base_dir_from_pattern(Path::new("**/*.rs")), Path::new("."));
    }

    #[test]
    fn base_dir_with_prefix() {
        assert_eq!(
            base_dir_from_pattern(Path::new("src/**/*.rs")),
            Path::new("src")
        );
    }

    #[test]
    fn base_dir_nested_prefix() {
        assert_eq!(
            base_dir_from_pattern(Path::new("src/foo/*.rs")),
            Path::new("src/foo")
        );
    }

    #[test]
    fn base_dir_no_separator_before_glob() {
        assert_eq!(base_dir_from_pattern(Path::new("*.rs")), Path::new("."));
    }

    // --- build_dir_matcher ---

    #[test]
    fn dir_matcher_flat_pattern_matches_nothing() {
        // "foo*" has no directory component; with literal_separator=true, * cannot
        // cross directory boundaries, so no subdirectory is ever relevant — not
        // even "foobar", since foo* matches only direct-child files.
        let m = build_dir_matcher(Path::new("foo*"));
        assert!(!m.is_match(Path::new("foobar")));
        assert!(!m.is_match(Path::new("target")));
        assert!(!m.is_match(Path::new(".")));
    }

    #[test]
    fn dir_matcher_single_star_allows_explicit_dir_only() {
        // "spec/basic/*.toml" should allow spec/ and spec/basic/ but nothing deeper
        let m = build_dir_matcher(Path::new("spec/basic/*.toml"));
        assert!(m.is_match(Path::new("spec")));
        assert!(m.is_match(Path::new("spec/basic")));
        assert!(!m.is_match(Path::new("spec/basic/sub")));
        assert!(!m.is_match(Path::new("other")));
    }

    #[test]
    fn dir_matcher_double_star_allows_all_subdirs() {
        // "spec/**/*.toml" → spec/ and any descendant
        let m = build_dir_matcher(Path::new("spec/**/*.toml"));
        assert!(m.is_match(Path::new("spec")));
        assert!(m.is_match(Path::new("spec/a")));
        assert!(m.is_match(Path::new("spec/a/b")));
        assert!(!m.is_match(Path::new("other")));
    }

    #[test]
    fn dir_matcher_bare_double_star_allows_everything() {
        // "**/*.toml" → any directory
        let m = build_dir_matcher(Path::new("**/*.toml"));
        assert!(m.is_match(Path::new("target")));
        assert!(m.is_match(Path::new("target/sub")));
        assert!(m.is_match(Path::new("spec")));
    }

    // --- walk ---

    fn setup_test_dir(name: &str, files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aureum_glob_test_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        for file in files {
            let path = dir.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "").unwrap();
        }
        dir
    }

    #[test]
    fn walk_double_star_finds_all_files() {
        let dir = setup_test_dir("walk_ds", &["a.toml", "sub/b.toml", "sub/deep/c.toml"]);
        let mut result = walk(&dir.join("**/*.toml")).unwrap();
        result.sort();
        assert_eq!(result.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn walk_single_star_stays_flat() {
        let dir = setup_test_dir("walk_ss", &["a.toml", "sub/b.toml"]);
        let result = walk(&dir.join("*.toml")).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].ends_with("a.toml"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- walk_entries ---

    #[test]
    fn walk_entries_matches_directories_with_separator() {
        let dir = setup_test_dir(
            "walk_entries_dir",
            &["aureum/src/a.rs", "aureum-cli/src/b.rs", "other/src/c.rs"],
        );
        let mut result = walk_entries(&dir.join("aureum*/src")).unwrap();
        result.sort_by_key(|e| match e {
            Entry::File(p) | Entry::Dir(p) => p.clone(),
        });
        assert_eq!(result.len(), 2, "{result:?}");
        assert!(result.iter().all(|e| matches!(e, Entry::Dir(_))));
        assert!(
            result
                .iter()
                .any(|e| matches!(e, Entry::Dir(p) if p.ends_with("aureum/src")))
        );
        assert!(
            result
                .iter()
                .any(|e| matches!(e, Entry::Dir(p) if p.ends_with("aureum-cli/src")))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn walk_entries_name_pattern_returns_dir_and_file() {
        let dir = setup_test_dir(
            "walk_entries_name",
            &["spec/a.toml", "spec.au.toml", "other/b.toml"],
        );
        let mut result = walk_entries(&dir.join("spec*")).unwrap();
        result.sort_by_key(|e| match e {
            Entry::File(p) | Entry::Dir(p) => p.clone(),
        });
        assert_eq!(result.len(), 2, "{result:?}");
        assert!(
            result
                .iter()
                .any(|e| matches!(e, Entry::Dir(p) if p.ends_with("spec")))
        );
        assert!(
            result
                .iter()
                .any(|e| matches!(e, Entry::File(p) if p.ends_with("spec.au.toml")))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
