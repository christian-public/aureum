//! The `SubtestPath` type is used to reference a specific test in a test file.
//!
//! Each level of a `SubtestPath` is separated by a `.` (dot).
//! The root node can be referenced using `SubtestPath::root()`.
use std::convert::TryFrom;
use std::fmt;

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct SubtestPath {
    id_path: Vec<String>,
}

impl SubtestPath {
    /// This method does no validation of the segments.
    /// If validation is necessary, call try_from() instead.
    pub fn new(id_path: Vec<impl Into<String>>) -> SubtestPath {
        SubtestPath {
            id_path: id_path.into_iter().map(Into::into).collect(),
        }
    }

    pub fn root() -> SubtestPath {
        Self::new(Vec::<String>::new())
    }

    pub fn id_path(self) -> Vec<String> {
        self.id_path
    }

    pub fn contains(&self, other: &SubtestPath) -> bool {
        if self.id_path.len() <= other.id_path.len() {
            self.id_path == other.id_path[..self.id_path.len()]
        } else {
            false
        }
    }

    pub fn is_root(&self) -> bool {
        self.id_path.is_empty()
    }
}

impl<'a> TryFrom<&'a str> for SubtestPath {
    type Error = ();

    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        if s.is_empty() {
            return Err(());
        }
        let segments: Vec<String> = s.split('.').map(String::from).collect();
        if segments.iter().all(|seg| is_valid_segment(seg)) {
            Ok(SubtestPath { id_path: segments })
        } else {
            Err(())
        }
    }
}

fn is_valid_segment(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

impl fmt::Display for SubtestPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id_path.join("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_path() {
        let root = SubtestPath::root();
        let root_level1 = SubtestPath::new(vec!["level1"]);

        assert_eq!(root.id_path(), Vec::<String>::new());
        assert_eq!(root_level1.id_path(), vec![String::from("level1")]);
    }

    #[test]
    fn test_to_string() {
        let root = SubtestPath::root();
        let root_level1 = SubtestPath::new(vec!["level1"]);
        let root_level1_level2 = SubtestPath::new(vec!["level1", "level2"]);

        assert_eq!(root.to_string(), "");
        assert_eq!(root_level1.to_string(), "level1");
        assert_eq!(root_level1_level2.to_string(), "level1.level2");
    }

    #[test]
    fn test_contains() {
        let root = SubtestPath::root();
        let root_level1 = SubtestPath::new(vec!["level1"]);
        let root_level1_level2 = SubtestPath::new(vec!["level1", "level2"]);

        assert!(root.contains(&root));
        assert!(root.contains(&root_level1));
        assert!(root.contains(&root_level1_level2));
        assert!(root_level1.contains(&root_level1));
        assert!(root_level1.contains(&root_level1_level2));
        assert!(root_level1_level2.contains(&root_level1_level2));

        assert!(!root_level1.contains(&root));
    }

    #[test]
    fn test_contains_for_distinct_levels() {
        let level1a = SubtestPath::new(vec!["level1a"]);
        let level1b = SubtestPath::new(vec!["level1b"]);

        assert!(!level1a.contains(&level1b));
        assert!(!level1b.contains(&level1a));
    }

    #[test]
    fn test_is_root() {
        let root = SubtestPath::root();
        let root_level1 = SubtestPath::new(vec!["level1"]);

        assert!(root.is_root());
        assert!(!root_level1.is_root());
    }

    #[test]
    fn test_try_from_valid() {
        assert_eq!(SubtestPath::try_from("a"), Ok(SubtestPath::new(vec!["a"])));
        assert_eq!(
            SubtestPath::try_from("test1"),
            Ok(SubtestPath::new(vec!["test1"]))
        );
        assert_eq!(
            SubtestPath::try_from("my_test"),
            Ok(SubtestPath::new(vec!["my_test"]))
        );
        assert_eq!(
            SubtestPath::try_from("my-test"),
            Ok(SubtestPath::new(vec!["my-test"]))
        );
        assert_eq!(
            SubtestPath::try_from("a.b"),
            Ok(SubtestPath::new(vec!["a", "b"]))
        );
        assert_eq!(
            SubtestPath::try_from("ABC123_-"),
            Ok(SubtestPath::new(vec!["ABC123_-"]))
        );
    }

    #[test]
    fn test_try_from_invalid() {
        assert!(SubtestPath::try_from("").is_err()); // empty
        assert!(SubtestPath::try_from("my test").is_err()); // space
        assert!(SubtestPath::try_from("test!").is_err()); // punctuation
        assert!(SubtestPath::try_from("тест").is_err()); // non-ASCII
        assert!(SubtestPath::try_from("foo..bar").is_err()); // empty segment from double-dot
        assert!(SubtestPath::try_from(".foo").is_err()); // leading dot → empty segment
        assert!(SubtestPath::try_from("foo.").is_err()); // trailing dot → empty segment
    }
}
