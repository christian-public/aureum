use crate::subtest_path::SubtestPath;

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct SubtestPathCoverageSet {
    subtest_paths: Vec<SubtestPath>,
}

impl SubtestPathCoverageSet {
    pub fn empty() -> SubtestPathCoverageSet {
        SubtestPathCoverageSet {
            subtest_paths: vec![],
        }
    }

    pub fn full() -> SubtestPathCoverageSet {
        SubtestPathCoverageSet {
            subtest_paths: vec![SubtestPath::root()],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.subtest_paths.is_empty()
    }

    pub fn len(&self) -> usize {
        self.subtest_paths.len()
    }

    pub fn contains(&self, subtest_path: &SubtestPath) -> bool {
        for existing_subtest_path in &self.subtest_paths {
            if existing_subtest_path.contains(subtest_path) {
                return true;
            }
        }

        false
    }

    pub fn add(&mut self, subtest_path: SubtestPath) -> bool {
        // Halt if the new element is already contained
        if self.contains(&subtest_path) {
            return false;
        }

        // Remove any elements that are contained by the new element
        self.subtest_paths
            .retain(|existing_subtest_path| !subtest_path.contains(existing_subtest_path));

        // Add new element and sort list
        self.subtest_paths.push(subtest_path);
        self.subtest_paths.sort();

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains() {
        let root = SubtestPath::root();
        let root_level1 = SubtestPath::new(vec!["level1"]);
        let root_level1_level2 = SubtestPath::new(vec!["level1", "level2"]);

        let mut coverage_set = SubtestPathCoverageSet::empty();

        assert!(!coverage_set.contains(&root));
        assert!(!coverage_set.contains(&root_level1));
        assert!(!coverage_set.contains(&root_level1_level2));

        assert!(coverage_set.add(root_level1_level2.clone()));

        assert!(!coverage_set.contains(&root));
        assert!(!coverage_set.contains(&root_level1));
        assert!(coverage_set.contains(&root_level1_level2));

        assert!(coverage_set.add(root.clone()));

        assert!(coverage_set.contains(&root));
        assert!(coverage_set.contains(&root_level1));
        assert!(coverage_set.contains(&root_level1_level2));
    }

    #[test]
    fn test_add_root_late_collapses_subtest_paths() {
        let root = SubtestPath::root();
        let root_level1a = SubtestPath::new(vec!["level1a"]);
        let root_level1b = SubtestPath::new(vec!["level1b"]);
        let root_level1c = SubtestPath::new(vec!["level1c"]);

        let mut coverage_set = SubtestPathCoverageSet::empty();

        assert_eq!(coverage_set.len(), 0);

        assert!(coverage_set.add(root_level1a));
        assert!(coverage_set.add(root_level1b));
        assert!(coverage_set.add(root_level1c));

        assert_eq!(coverage_set.len(), 3);

        assert!(coverage_set.add(root));

        assert_eq!(coverage_set.len(), 1);
    }

    #[test]
    fn test_add_root_early_blocks_subtest_paths() {
        let root = SubtestPath::root();
        let root_level1 = SubtestPath::new(vec!["level1"]);

        let mut coverage_set = SubtestPathCoverageSet::empty();

        assert_eq!(coverage_set.len(), 0);

        assert!(coverage_set.add(root));
        assert!(!coverage_set.add(root_level1));

        assert_eq!(coverage_set.len(), 1);
    }
}
