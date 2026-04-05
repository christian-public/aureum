//! The `TestId` type is used to reference a specific test in a test file.
//!
//! Each level of a `TestId` is separated by a `.` (dot).
//! The root node can be referenced using `TestId::root()`.
//!
//! # Examples
//!
//! ```
//! use aureum::TestId;
//! let example = TestId::from("level1.level2");
//! assert_eq!(format!("{example}"), "level1.level2");
//! ```
use std::fmt;

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestId {
    id_path: Vec<String>,
}

impl TestId {
    pub fn new(id_path: Vec<String>) -> TestId {
        TestId { id_path }
    }

    pub fn root() -> TestId {
        Self::new(vec![])
    }

    pub fn from(str: &str) -> TestId {
        if str.is_empty() {
            TestId { id_path: vec![] }
        } else {
            TestId {
                id_path: str.split('.').map(String::from).collect(),
            }
        }
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

impl fmt::Display for TestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id_path.join("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_vs_from() {
        let root1 = TestId::new(vec![]);
        let root2 = TestId::from("");

        let test1 = TestId::new(vec![String::from("test")]);
        let test2 = TestId::from("test");

        let two_levels1 = TestId::new(vec![String::from("level1"), String::from("level2")]);
        let two_levels2 = TestId::from("level1.level2");

        assert!(root1 == root2);
        assert!(test1 == test2);
        assert!(two_levels1 == two_levels2);
    }

    #[test]
    fn test_id_path() {
        let root = TestId::from("");
        let root_level1 = TestId::from("level1");

        assert_eq!(root.id_path(), Vec::<String>::new());
        assert_eq!(root_level1.id_path(), vec![String::from("level1")]);
    }

    #[test]
    fn test_to_string() {
        let root = TestId::from("");
        let root_level1 = TestId::from("level1");
        let root_level1_level2 = TestId::from("level1.level2");

        assert_eq!(root.to_string(), "");
        assert_eq!(root_level1.to_string(), "level1");
        assert_eq!(root_level1_level2.to_string(), "level1.level2");
    }

    #[test]
    fn test_contains() {
        let root = TestId::from("");
        let root_level1 = TestId::from("level1");
        let root_level1_level2 = TestId::from("level1.level2");

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
        let level1a = TestId::from("level1a");
        let level1b = TestId::from("level1b");

        assert!(!level1a.contains(&level1b));
        assert!(!level1b.contains(&level1a));
    }

    #[test]
    fn test_is_root() {
        let root = TestId::root();
        let root_level1 = TestId::from("level1");

        assert!(root.is_root());
        assert!(!root_level1.is_root());
    }
}
