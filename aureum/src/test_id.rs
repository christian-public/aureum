//! The `TestId` type is used to reference a specific test in a test file.
//!
//! Each level of a `TestId` is separated by a `.` (dot).
//! The root node can be referenced using `TestId::root()`.
use std::convert::TryFrom;
use std::fmt;

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestId {
    id_path: Vec<String>,
}

impl TestId {
    /// This method does no validation of the segments.
    /// If validation is necessary, call try_from() instead.
    pub fn new(id_path: Vec<impl Into<String>>) -> TestId {
        TestId {
            id_path: id_path.into_iter().map(Into::into).collect(),
        }
    }

    pub fn root() -> TestId {
        Self::new(Vec::<String>::new())
    }

    pub fn id_path(self) -> Vec<String> {
        self.id_path
    }

    pub fn contains(&self, other: &TestId) -> bool {
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

impl<'a> TryFrom<&'a str> for TestId {
    type Error = ();

    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        if s.is_empty() {
            return Err(());
        }
        let segments: Vec<String> = s.split('.').map(String::from).collect();
        if segments.iter().all(|seg| is_valid_segment(seg)) {
            Ok(TestId { id_path: segments })
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

impl fmt::Display for TestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id_path.join("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_path() {
        let root = TestId::root();
        let root_level1 = TestId::new(vec!["level1"]);

        assert_eq!(root.id_path(), Vec::<String>::new());
        assert_eq!(root_level1.id_path(), vec![String::from("level1")]);
    }

    #[test]
    fn test_to_string() {
        let root = TestId::root();
        let root_level1 = TestId::new(vec!["level1"]);
        let root_level1_level2 = TestId::new(vec!["level1", "level2"]);

        assert_eq!(root.to_string(), "");
        assert_eq!(root_level1.to_string(), "level1");
        assert_eq!(root_level1_level2.to_string(), "level1.level2");
    }

    #[test]
    fn test_contains() {
        let root = TestId::root();
        let root_level1 = TestId::new(vec!["level1"]);
        let root_level1_level2 = TestId::new(vec!["level1", "level2"]);

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
        let level1a = TestId::new(vec!["level1a"]);
        let level1b = TestId::new(vec!["level1b"]);

        assert!(!level1a.contains(&level1b));
        assert!(!level1b.contains(&level1a));
    }

    #[test]
    fn test_is_root() {
        let root = TestId::root();
        let root_level1 = TestId::new(vec!["level1"]);

        assert!(root.is_root());
        assert!(!root_level1.is_root());
    }

    #[test]
    fn test_try_from_valid() {
        assert_eq!(TestId::try_from("a"), Ok(TestId::new(vec!["a"])));
        assert_eq!(TestId::try_from("test1"), Ok(TestId::new(vec!["test1"])));
        assert_eq!(
            TestId::try_from("my_test"),
            Ok(TestId::new(vec!["my_test"]))
        );
        assert_eq!(
            TestId::try_from("my-test"),
            Ok(TestId::new(vec!["my-test"]))
        );
        assert_eq!(TestId::try_from("a.b"), Ok(TestId::new(vec!["a", "b"])));
        assert_eq!(
            TestId::try_from("ABC123_-"),
            Ok(TestId::new(vec!["ABC123_-"]))
        );
    }

    #[test]
    fn test_try_from_invalid() {
        assert!(TestId::try_from("").is_err()); // empty
        assert!(TestId::try_from("my test").is_err()); // space
        assert!(TestId::try_from("test!").is_err()); // punctuation
        assert!(TestId::try_from("тест").is_err()); // non-ASCII
        assert!(TestId::try_from("foo..bar").is_err()); // empty segment from double-dot
        assert!(TestId::try_from(".foo").is_err()); // leading dot → empty segment
        assert!(TestId::try_from("foo.").is_err()); // trailing dot → empty segment
    }
}
