use crate::test_id::TestId;

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestIdCoverageSet {
    test_ids: Vec<TestId>,
}

impl TestIdCoverageSet {
    pub fn empty() -> TestIdCoverageSet {
        TestIdCoverageSet { test_ids: vec![] }
    }

    pub fn full() -> TestIdCoverageSet {
        TestIdCoverageSet {
            test_ids: vec![TestId::root()],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.test_ids.is_empty()
    }

    pub fn len(&self) -> usize {
        self.test_ids.len()
    }

    pub fn add(&mut self, test_id: TestId) -> bool {
        // Halt if the new element is already contained
        for existing_test_id in &self.test_ids {
            if existing_test_id.contains(&test_id) {
                return false;
            }
        }

        // Remove any elements that are contained by the new element
        self.test_ids
            .retain(|existing_test_id| !test_id.contains(existing_test_id));

        // Add new element and sort list
        self.test_ids.push(test_id);
        self.test_ids.sort();

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_root_late_collapses_test_ids() {
        let root = TestId::root();
        let root_level1a = TestId::from("level1a");
        let root_level1b = TestId::from("level1b");
        let root_level1c = TestId::from("level1c");

        let mut coverage_set = TestIdCoverageSet::empty();

        assert_eq!(coverage_set.len(), 0);
        assert_eq!(coverage_set.add(root_level1a), true);
        assert_eq!(coverage_set.add(root_level1b), true);
        assert_eq!(coverage_set.add(root_level1c), true);
        assert_eq!(coverage_set.len(), 3);
        assert_eq!(coverage_set.add(root), true);
        assert_eq!(coverage_set.len(), 1);
    }

    #[test]
    fn test_add_root_early_blocks_test_ids() {
        let root = TestId::root();
        let root_level1 = TestId::from("level1");

        let mut coverage_set = TestIdCoverageSet::empty();

        assert_eq!(coverage_set.len(), 0);
        assert_eq!(coverage_set.add(root), true);
        assert_eq!(coverage_set.add(root_level1), false);
        assert_eq!(coverage_set.len(), 1);
    }
}
