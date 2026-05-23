use std::collections::BTreeMap;
use std::path::PathBuf;

/// Materialisation plan for a single test's per-test scratch directory.
/// The runner uses this to create files before launching the test program.
#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct ScratchPlan {
    /// Absolute path of the per-test scratch directory.
    pub dir: PathBuf,
    /// Files to copy from disk into the scratch dir before running the test.
    pub copies: Vec<FileCopy>,
    /// Inline embed files to write into the scratch dir before running the test.
    pub embeds: Vec<EmbedWrite>,
    /// When `true`, the runner also leaves an `aureum-rerun.sh` (and stdin
    /// sidecar) in the scratch dir. Only worth doing when the dir survives the
    /// run, so it tracks `--keep-scratch`. Defaults to `false`.
    pub write_rerun_script: bool,
}

/// Scratch settings supplied by the CLI for a whole run. Bundles the scratch
/// root with the `--keep-scratch` flag.
#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct ScratchConfig {
    /// Root directory under which per-test scratch dirs are created.
    pub root: PathBuf,
    /// When `true`, the runner leaves an `aureum-rerun.sh` in each per-test
    /// dir. Paired with `--keep-scratch`, which preserves those dirs.
    pub write_rerun_script: bool,
}

/// Per-test scratch destination, derived from a [`ScratchConfig`] once the
/// test's discovery position and id are known. Carries the resolved per-test
/// dir alongside the rerun-script flag through the validation layer, so the
/// flag rides as a typed field rather than a separate threaded argument.
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct ScratchTarget {
    /// Absolute path of this test's per-test scratch dir.
    pub dir: PathBuf,
    /// Propagated to [`ScratchPlan::write_rerun_script`].
    pub write_rerun_script: bool,
}

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct FileCopy {
    /// Absolute source path on disk.
    pub source: PathBuf,
    /// Scratch-relative destination path.
    pub dest_relative: String,
}

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct EmbedWrite {
    /// Scratch-relative destination path.
    pub dest_relative: String,
    pub content: String,
}

/// Errors emitted by `ScratchBuilder` while planning a per-test scratch
/// directory. Callers in the validator layer translate these into the
/// crate-wide `ValidationError` variants.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum ScratchPlanError {
    /// The supplied scratch-relative path is not safe (empty, absolute,
    /// contains `..`, etc.).
    InvalidPath(String),
    /// The source file referenced by a copy does not exist on disk.
    MissingSourceFile(String),
    /// Two sources would write to the same scratch-relative destination.
    PathConflict(String),
    /// A `path_of_embed` reference names an embed that wasn't declared.
    EmbedUnknown(String),
}

/// Cap on the sanitised-id portion of the per-test dir name. Picked to keep
/// the assembled path `<scratch-root>/<this name>/<copy-or-embed dest>` well
/// under the strictest PATH_MAX we care about (260 on Windows): 120 bytes
/// here leaves ~140 bytes of headroom for the scratch root and the
/// test-relative subpath. Far below per-component filesystem limits
/// (typically 255 bytes on ext4/APFS/NTFS).
const SANITISED_ID_BUDGET: usize = 120;

/// Compute the per-test directory name under the scratch root, in the form
/// `aureum-{position}--<sanitised id>`. The `aureum-` prefix marks the name
/// unambiguously aureum-owned so cleanup can't mistake a user's own files
/// (e.g. `2024-…`, `0001-migration.sql`) for a per-test dir.
///
/// Separator hierarchy inside the assembled name:
/// - `--` denotes a *major boundary*: framing↔id, and path↔subtest within
///   the id. Always exactly two dashes; never produced incidentally.
/// - `-` denotes a *path-segment join* inside the id.
/// - `_` is segment-internal: any non-alphanumeric character in a path
///   segment (`.`, `-`, whitespace, unicode, …) is mapped to `_`. So a
///   kebab-case segment like `my-cool-app` becomes `my_cool_app`, which
///   reads as one word under double-click selection.
///
/// `position` is the test's 1-based index in canonical discovery order
/// across all loaded config files. Globally unique by construction, which
/// means the sanitised id can be aggressively front-truncated for
/// readability without any risk of cross-test collisions.
pub fn per_test_dir_name(position: usize, test_id_display: &str) -> String {
    let sanitised = sanitise_id(test_id_display, SANITISED_ID_BUDGET);
    format!("aureum-{position:04}--{sanitised}")
}

/// Returns true if `name` matches the per-test directory naming scheme
/// produced by [`per_test_dir_name`]: `aureum-` + digits + `--` + non-empty
/// remainder. The digit run is matched as *any* non-empty width — padding
/// is cosmetic, and Rust's formatter widens rather than truncating on
/// overflow, so matching any width keeps cleanup correct as the suite
/// grows. The required `--` (rather than a single `-`) is what makes this
/// reliable: it's a strong machine-generated signature unlikely to collide
/// with anything a user might keep under their `--scratch-root`.
pub fn is_per_test_dir_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("aureum-") else {
        return false;
    };
    let Some(dash_idx) = rest.find('-') else {
        return false;
    };
    if dash_idx == 0 || !rest.as_bytes()[..dash_idx].iter().all(u8::is_ascii_digit) {
        return false;
    }
    // Require the structural `--` (not just a single `-`) and at least one
    // character of sanitised id after it.
    let after_digits = &rest[dash_idx..];
    let Some(remainder) = after_digits.strip_prefix("--") else {
        return false;
    };
    !remainder.is_empty()
}

/// Sanitise a single segment (between structural `/` or `:` boundaries) of
/// the display id. Keeps `[A-Za-z0-9]`; everything else — including `-`,
/// `.`, whitespace, unicode — becomes `_`. Then collapses runs of `_` and
/// trims leading/trailing `_`.
///
/// Mapping the original `-` characters to `_` keeps segment-internal
/// separators visually tight: double-clicking selects the whole segment as
/// one "word" because `_` is a word character in most editors while `-`
/// breaks a word. Segments are joined with `-` (see [`sanitise_id`]), so
/// the resulting name reads as `path-segments-joined_with-underscores`
/// where each kebab piece corresponds to one original path component.
///
/// A side benefit: because segments cannot contain `-`, the structural
/// `--` marker is unambiguous by construction — no need to collapse runs
/// of `-`, since none can occur inside a segment.
fn sanitise_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_was_underscore = false;
    for c in s.chars() {
        let mapped = if c.is_ascii_alphanumeric() { c } else { '_' };
        if mapped == '_' && last_was_underscore {
            continue;
        }
        out.push(mapped);
        last_was_underscore = mapped == '_';
    }
    while out.starts_with('_') {
        out.remove(0);
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}

/// Build the sanitised id with tail-first byte budgeting.
///
/// The display id has shape `<dir>/.../<file_name>[:<subtest>]`. The
/// rightmost parts (subtest, then file name) carry the most discriminating
/// information, so the id is assembled from the tail. If the full id
/// exceeds `budget`, path-prefix segments are dropped from the front first;
/// the subtest is never sacrificed. In the pathological case where the
/// subtest alone exceeds the budget, it is front-truncated.
///
/// Output never exceeds `budget` bytes. Output is always non-empty when the
/// input is non-empty.
fn sanitise_id(display_id: &str, budget: usize) -> String {
    let (path_part, subtest_part) = match display_id.split_once(':') {
        Some((p, s)) => (p, Some(s)),
        None => (display_id, None),
    };
    let mut path_segments: Vec<&str> = path_part.split('/').collect();
    // The last path segment is the file name; everything else is the prefix.
    let file_name_raw = path_segments.pop().unwrap_or("");
    let file_name_san = sanitise_segment(file_name_raw);
    let path_prefix_san: Vec<String> = path_segments
        .iter()
        .map(|s| sanitise_segment(s))
        .filter(|s| !s.is_empty())
        .collect();
    let subtest_san = subtest_part.map(sanitise_segment);

    // Pieces are assembled from tail (rightmost in final string) to head.
    // Each entry is (segment, separator-when-prepended). The separator for
    // the very first piece is unused.
    let mut pieces: Vec<(String, &'static str)> = Vec::new();
    match subtest_san {
        Some(s) if !s.is_empty() => {
            pieces.push((s, ""));
            pieces.push((file_name_san, "--"));
        }
        _ => {
            pieces.push((file_name_san, ""));
        }
    }
    for seg in path_prefix_san.into_iter().rev() {
        pieces.push((seg, "-"));
    }

    let mut iter = pieces.into_iter();
    let (tail, _) = iter.next().expect("at least one piece (the file name)");
    // If the tail alone exceeds budget, front-truncate it. Sanitised output
    // is ASCII (only `[A-Za-z0-9_-]`), so byte-slicing respects char
    // boundaries.
    let mut result = if tail.len() > budget {
        tail[tail.len() - budget..].to_owned()
    } else {
        tail
    };
    let mut remaining = budget.saturating_sub(result.len());
    for (seg, sep) in iter {
        let needed = seg.len() + sep.len();
        if needed <= remaining {
            let mut next = String::with_capacity(result.len() + needed);
            next.push_str(&seg);
            next.push_str(sep);
            next.push_str(&result);
            result = next;
            remaining -= needed;
        } else {
            // Don't half-include a section; stop here. Discriminating tail
            // is already in `result`, position prefix guarantees uniqueness.
            break;
        }
    }
    result
}

/// Validate that a scratch-relative path (embed destination or `path_of_file`
/// reference) is safe to materialise inside a scratch dir: relative, non-empty,
/// contains no `..` segments, and does not start with `/` or `\`.
pub fn is_valid_scratch_path(p: &str) -> bool {
    if p.is_empty() {
        return false;
    }
    if p.starts_with('/') || p.starts_with('\\') {
        return false;
    }
    // Reject any backslash too — paths inside the config are always written
    // with forward slashes, and `\` segments would be confusing on Windows.
    if p.contains('\\') {
        return false;
    }
    for segment in p.split('/') {
        if segment.is_empty() || segment == ".." || segment == "." {
            return false;
        }
    }
    true
}

// EMBED REGISTRY

/// File-scoped registry of resolved embed contents, keyed by declared path.
/// Populated by the validator (which reads embed content from TOML), then
/// consumed by `ScratchBuilder` when resolving `path_of_embed` references.
#[derive(Default)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct EmbedRegistry {
    map: BTreeMap<String, String>,
}

impl EmbedRegistry {
    pub fn contains(&self, path: &str) -> bool {
        self.map.contains_key(path)
    }

    pub fn insert(&mut self, path: String, content: String) {
        self.map.insert(path, content);
    }

    pub fn get(&self, path: &str) -> Option<&str> {
        self.map.get(path).map(String::as_str)
    }
}

// SCRATCH BUILDER

/// Per-test scratch planner. Accumulates copy/embed operations while string
/// value sources are resolved; once finalised it yields a `ScratchPlan`.
pub struct ScratchBuilder<'a> {
    per_test_dir: PathBuf,
    config_dir: PathBuf,
    embeds: &'a EmbedRegistry,
    write_rerun_script: bool,
    /// Scratch-relative destination paths claimed so far, mapped to the source
    /// description that claimed them (used to detect conflicts).
    claimed: BTreeMap<String, ClaimSource>,
    copies: Vec<FileCopy>,
    embed_writes: Vec<EmbedWrite>,
}

#[derive(Clone)]
enum ClaimSource {
    Embed,
    Copy { source: PathBuf },
}

impl<'a> ScratchBuilder<'a> {
    pub fn new(
        per_test_dir: PathBuf,
        config_dir: PathBuf,
        embeds: &'a EmbedRegistry,
        write_rerun_script: bool,
    ) -> Self {
        Self {
            per_test_dir,
            config_dir,
            embeds,
            write_rerun_script,
            claimed: BTreeMap::new(),
            copies: Vec::new(),
            embed_writes: Vec::new(),
        }
    }

    /// Plan a copy of `path_of_file` into the scratch dir and return the
    /// scratch-relative destination for substitution into
    /// `program_arguments`/`stdin`. The test runs with `cwd` set to the
    /// scratch dir, so a scratch-relative path resolves correctly there and
    /// keeps the substituted value stable across hosts (no absolute paths).
    pub fn plan_copy(&mut self, file_path: &str) -> Result<String, ScratchPlanError> {
        self.claim_copy(file_path)?;
        Ok(file_path.to_owned())
    }

    /// Plan a copy of an `input_files` entry. Unlike `plan_copy`, this is a
    /// pure side effect — there is no string value being substituted into the
    /// test's `program_arguments`/`stdin`, so no return path is needed.
    pub fn add_input_file(&mut self, rel_path: &str) -> Result<(), ScratchPlanError> {
        self.claim_copy(rel_path)
    }

    /// Shared implementation behind `plan_copy` and `add_input_file`.
    /// Validates the path, confirms the source exists, and records the claim
    /// (rejecting conflicting destinations and merging duplicate identical
    /// copies).
    fn claim_copy(&mut self, rel_path: &str) -> Result<(), ScratchPlanError> {
        if !is_valid_scratch_path(rel_path) {
            return Err(ScratchPlanError::InvalidPath(rel_path.to_owned()));
        }
        let source = self.config_dir.join(rel_path);
        if !source.exists() {
            return Err(ScratchPlanError::MissingSourceFile(rel_path.to_owned()));
        }
        let dest_relative = rel_path.to_owned();

        match self.claimed.get(&dest_relative).cloned() {
            // Same `path_of_file` referenced more than once in the same test: idempotent.
            Some(ClaimSource::Copy { source: existing }) if existing == source => {}
            Some(_) => return Err(ScratchPlanError::PathConflict(dest_relative)),
            None => {
                self.claimed.insert(
                    dest_relative.clone(),
                    ClaimSource::Copy {
                        source: source.clone(),
                    },
                );
                self.copies.push(FileCopy {
                    source,
                    dest_relative,
                });
            }
        }
        Ok(())
    }

    pub fn resolve_embed(&mut self, embed_path: &str) -> Result<String, ScratchPlanError> {
        let Some(content) = self.embeds.get(embed_path) else {
            return Err(ScratchPlanError::EmbedUnknown(embed_path.to_owned()));
        };
        let dest_relative = embed_path.to_owned();

        match self.claimed.get(&dest_relative).cloned() {
            Some(ClaimSource::Embed) => {
                // Same embed referenced twice in the same test: OK, we already wrote it.
                Ok(dest_relative)
            }
            Some(ClaimSource::Copy { .. }) => Err(ScratchPlanError::PathConflict(dest_relative)),
            None => {
                self.claimed
                    .insert(dest_relative.clone(), ClaimSource::Embed);
                self.embed_writes.push(EmbedWrite {
                    dest_relative: dest_relative.clone(),
                    content: content.to_owned(),
                });
                Ok(dest_relative)
            }
        }
    }

    pub fn finish(self) -> ScratchPlan {
        ScratchPlan {
            dir: self.per_test_dir,
            copies: self.copies,
            embeds: self.embed_writes,
            write_rerun_script: self.write_rerun_script,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitise_segment_keeps_alphanumerics_only() {
        // `-` is no longer preserved: it's mapped to `_` so that segments
        // can be joined with `-` and still read as one "word" per segment
        // under double-click selection (which treats `_` as part of a word
        // but breaks on `-`).
        assert_eq!(sanitise_segment("echo-test-2"), "echo_test_2");
    }

    #[test]
    fn sanitise_segment_replaces_dot_with_underscore() {
        // `.` is not preserved — `echo.au.toml` becomes `echo_au_toml`,
        // avoiding a dir name that looks like a file extension and playing
        // nicer with glob-based tooling.
        assert_eq!(sanitise_segment("echo.au.toml"), "echo_au_toml");
    }

    #[test]
    fn sanitise_segment_collapses_underscore_runs_and_trims() {
        // Multiple non-alphanumerics in a row collapse to a single `_`.
        // Trim ensures `:hello:` doesn't leak boundary marks. There's no
        // `-` collapse rule because segments can't contain `-` at all
        // (the kebab in the input is mapped to `_`).
        assert_eq!(sanitise_segment("__foo  bar__"), "foo_bar");
        assert_eq!(sanitise_segment("foo---bar"), "foo_bar");
    }

    #[test]
    fn sanitise_segment_empty_input_yields_empty() {
        assert_eq!(sanitise_segment(""), "");
        assert_eq!(sanitise_segment("___"), "");
        assert_eq!(sanitise_segment("---"), "");
    }

    #[test]
    fn sanitise_id_path_only() {
        // No subtest: file name is the tail; path segments are joined
        // with `-`. No `--` because there's no path/subtest boundary.
        assert_eq!(
            sanitise_id("golden/basic/echo.au.toml", 200),
            "golden-basic-echo_au_toml"
        );
    }

    #[test]
    fn sanitise_id_with_subtest() {
        // `--` separates the path from the subtest. Path segments join
        // with `-`; segment-internal non-alphanumerics (`.`) become `_`.
        // So `echo.au.toml` → `echo_au_toml` reads as one word.
        assert_eq!(
            sanitise_id("golden/basic/echo.au.toml:sub.test", 200),
            "golden-basic-echo_au_toml--sub_test"
        );
    }

    #[test]
    fn sanitise_id_kebab_path_segments_become_one_word_each() {
        // A segment like `my-cool-app` is a single logical name, so its
        // internal `-` becomes `_`. The structural `-` between segments
        // is the only `-` in the assembled id.
        assert_eq!(
            sanitise_id("my-cool-app/test-suite/file.toml", 200),
            "my_cool_app-test_suite-file_toml"
        );
    }

    #[test]
    fn sanitise_id_keeps_full_assembly_when_under_budget() {
        // Full assembly is `a-b-c-d-file_toml--sub` (22 chars), which
        // fits in 25 — nothing is dropped. Path segments join with `-`,
        // path-to-subtest boundary is `--`.
        let id = sanitise_id("a/b/c/d/file.toml:sub", 25);
        assert_eq!(id, "a-b-c-d-file_toml--sub");
    }

    #[test]
    fn sanitise_id_drops_path_when_tight() {
        // Budget too small to include the prefix. Tail (filename + subtest)
        // is preserved; prefix segments are skipped from the front.
        let id = sanitise_id("very/long/prefix/path/file.toml:sub", 20);
        assert!(id.ends_with("file_toml--sub"));
        assert!(id.len() <= 20);
    }

    #[test]
    fn sanitise_id_front_truncates_when_tail_alone_too_big() {
        // Pathological case: even the tail (subtest) doesn't fit. Front-
        // truncate to keep the most-recently-distinguishing suffix. Don't
        // panic, don't return empty.
        let long_subtest = "a".repeat(50);
        let id = sanitise_id(&format!("file.toml:{long_subtest}"), 10);
        assert_eq!(id.len(), 10);
        assert!(id.chars().all(|c| c == 'a'));
    }

    #[test]
    fn per_test_dir_name_format() {
        assert_eq!(
            per_test_dir_name(7, "tests/echo.au.toml:hello"),
            "aureum-0007--tests-echo_au_toml--hello"
        );
    }

    #[test]
    fn per_test_dir_name_root_test() {
        // Root tests (no subtest) have no `--` inside the id portion;
        // only the framing `--` between position and id is present.
        assert_eq!(
            per_test_dir_name(1, "tests/echo.au.toml"),
            "aureum-0001--tests-echo_au_toml"
        );
    }

    #[test]
    fn is_per_test_dir_name_recognises_generated_names() {
        assert!(is_per_test_dir_name("aureum-0001--anything"));
        assert!(is_per_test_dir_name("aureum-9999--x"));
        assert!(is_per_test_dir_name(&per_test_dir_name(42, "some/test:id")));
    }

    #[test]
    fn is_per_test_dir_name_accepts_any_digit_width() {
        // Padding is purely cosmetic; matcher must accept any non-empty
        // run of digits so cleanup stays correct if positions ever exceed
        // the format string's 4-digit width.
        assert!(is_per_test_dir_name("aureum-1--x"));
        assert!(is_per_test_dir_name("aureum-12345--x"));
        assert!(is_per_test_dir_name("aureum-9999999999--x"));
    }

    #[test]
    fn is_per_test_dir_name_requires_double_dash() {
        // Names with a single `-` after the digits no longer match. This
        // is the deliberate tightening — `aureum-0001-foo` is now treated
        // as a user-owned dir during cleanup, not an aureum-owned one.
        assert!(!is_per_test_dir_name("aureum-0001-foo"));
        assert!(!is_per_test_dir_name("aureum-9999-x"));
    }

    #[test]
    fn is_per_test_dir_name_rejects_non_generated_names() {
        assert!(!is_per_test_dir_name(""));
        // Reject names without the `aureum-` prefix.
        assert!(!is_per_test_dir_name("0001--anything"));
        assert!(!is_per_test_dir_name("2024-q4-reports"));
        assert!(!is_per_test_dir_name("9999--x"));
        // Reject names with the prefix but no digit-then-`--` sequence.
        assert!(!is_per_test_dir_name("aureum-"));
        assert!(!is_per_test_dir_name("aureum-0001"));
        assert!(!is_per_test_dir_name("aureum-0001--"));
        assert!(!is_per_test_dir_name("aureum-0001a--foo"));
        // Non-digit immediately after the prefix means no digit run.
        assert!(!is_per_test_dir_name("aureum-abcd--text"));
        // Mixed digits + non-digits before the dash isn't a pure digit run.
        assert!(!is_per_test_dir_name("aureum-12a4--text"));
        // Reject names that don't start with the prefix.
        assert!(!is_per_test_dir_name("user-file.txt"));
        assert!(!is_per_test_dir_name("aureumX-0001--foo"));
    }

    #[test]
    fn scratch_path_validation() {
        assert!(is_valid_scratch_path("input.txt"));
        assert!(is_valid_scratch_path("subdir/input.txt"));
        assert!(!is_valid_scratch_path(""));
        assert!(!is_valid_scratch_path("/abs/path"));
        assert!(!is_valid_scratch_path("../escape"));
        assert!(!is_valid_scratch_path("subdir/../escape"));
        assert!(!is_valid_scratch_path("./input.txt"));
        assert!(!is_valid_scratch_path("a//b"));
        assert!(!is_valid_scratch_path("a\\b"));
    }

    // SCRATCH BUILDER

    /// Fixture: a config dir with one staged source file and an embed registry
    /// containing one named embed. Returned builder writes into `/tmp/scratch`
    /// (a fictional per-test dir; nothing is actually written during planning).
    fn fixture() -> (tempfile::TempDir, EmbedRegistry) {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("input.txt"), "x").unwrap();
        std::fs::write(dir.path().join("other.txt"), "y").unwrap();
        let mut embeds = EmbedRegistry::default();
        embeds.insert("inline.txt".to_owned(), "inline body".to_owned());
        (dir, embeds)
    }

    fn make_builder<'a>(
        config_dir: &std::path::Path,
        embeds: &'a EmbedRegistry,
    ) -> ScratchBuilder<'a> {
        ScratchBuilder::new(
            PathBuf::from("/tmp/scratch"),
            config_dir.to_path_buf(),
            embeds,
            false,
        )
    }

    #[test]
    fn plan_copy_returns_scratch_relative_path() {
        let (dir, embeds) = fixture();
        let mut b = make_builder(dir.path(), &embeds);
        let dest = b.plan_copy("input.txt").unwrap();
        assert_eq!(dest, "input.txt");
        let plan = b.finish();
        assert_eq!(plan.copies.len(), 1);
        assert_eq!(plan.copies[0].dest_relative, "input.txt");
        assert_eq!(plan.copies[0].source, dir.path().join("input.txt"));
    }

    #[test]
    fn plan_copy_same_path_twice_is_idempotent() {
        let (dir, embeds) = fixture();
        let mut b = make_builder(dir.path(), &embeds);
        let d1 = b.plan_copy("input.txt").unwrap();
        let d2 = b.plan_copy("input.txt").unwrap();
        assert_eq!(d1, d2);
        assert_eq!(b.finish().copies.len(), 1, "should not duplicate the copy");
    }

    #[test]
    fn plan_copy_rejects_invalid_path() {
        let (dir, embeds) = fixture();
        let mut b = make_builder(dir.path(), &embeds);
        let err = b.plan_copy("../escape").unwrap_err();
        assert_eq!(err, ScratchPlanError::InvalidPath("../escape".to_owned()));
    }

    #[test]
    fn plan_copy_rejects_missing_source() {
        let (dir, embeds) = fixture();
        let mut b = make_builder(dir.path(), &embeds);
        let err = b.plan_copy("does-not-exist.txt").unwrap_err();
        assert_eq!(
            err,
            ScratchPlanError::MissingSourceFile("does-not-exist.txt".to_owned())
        );
    }

    #[test]
    fn plan_copy_after_embed_at_same_path_conflicts() {
        let (dir, mut embeds) = fixture();
        embeds.insert("input.txt".to_owned(), "embed body".to_owned());
        let mut b = make_builder(dir.path(), &embeds);
        b.resolve_embed("input.txt").unwrap();
        let err = b.plan_copy("input.txt").unwrap_err();
        assert_eq!(err, ScratchPlanError::PathConflict("input.txt".to_owned()));
    }

    #[test]
    fn add_input_file_after_embed_at_same_path_conflicts() {
        let (dir, mut embeds) = fixture();
        embeds.insert("input.txt".to_owned(), "embed body".to_owned());
        let mut b = make_builder(dir.path(), &embeds);
        b.resolve_embed("input.txt").unwrap();
        let err = b.add_input_file("input.txt").unwrap_err();
        assert_eq!(err, ScratchPlanError::PathConflict("input.txt".to_owned()));
    }

    #[test]
    fn add_input_file_then_plan_copy_is_idempotent() {
        let (dir, embeds) = fixture();
        let mut b = make_builder(dir.path(), &embeds);
        b.add_input_file("input.txt").unwrap();
        let dest = b.plan_copy("input.txt").unwrap();
        assert_eq!(dest, "input.txt");
        assert_eq!(b.finish().copies.len(), 1);
    }

    #[test]
    fn resolve_embed_returns_scratch_relative_path() {
        let (dir, embeds) = fixture();
        let mut b = make_builder(dir.path(), &embeds);
        let dest = b.resolve_embed("inline.txt").unwrap();
        assert_eq!(dest, "inline.txt");
        let plan = b.finish();
        assert_eq!(plan.embeds.len(), 1);
        assert_eq!(plan.embeds[0].dest_relative, "inline.txt");
        assert_eq!(plan.embeds[0].content, "inline body");
    }

    #[test]
    fn resolve_embed_same_path_twice_is_idempotent() {
        let (dir, embeds) = fixture();
        let mut b = make_builder(dir.path(), &embeds);
        let d1 = b.resolve_embed("inline.txt").unwrap();
        let d2 = b.resolve_embed("inline.txt").unwrap();
        assert_eq!(d1, d2);
        assert_eq!(b.finish().embeds.len(), 1, "should not duplicate the embed");
    }

    #[test]
    fn resolve_embed_after_copy_at_same_path_conflicts() {
        let (dir, mut embeds) = fixture();
        embeds.insert("input.txt".to_owned(), "embed body".to_owned());
        let mut b = make_builder(dir.path(), &embeds);
        b.plan_copy("input.txt").unwrap();
        let err = b.resolve_embed("input.txt").unwrap_err();
        assert_eq!(err, ScratchPlanError::PathConflict("input.txt".to_owned()));
    }

    #[test]
    fn resolve_embed_rejects_unknown_embed_name() {
        let (dir, embeds) = fixture();
        let mut b = make_builder(dir.path(), &embeds);
        let err = b.resolve_embed("unknown.txt").unwrap_err();
        assert_eq!(
            err,
            ScratchPlanError::EmbedUnknown("unknown.txt".to_owned())
        );
    }
}
