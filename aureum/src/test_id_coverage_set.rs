use crate::test_id::TestId;

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestIdCoverageSet {
    ids: Vec<TestId>,
}

impl TestIdCoverageSet {
    pub fn empty() -> TestIdCoverageSet {
        TestIdCoverageSet { ids: vec![] }
    }

    pub fn full() -> TestIdCoverageSet {
        TestIdCoverageSet {
            ids: vec![TestId::root()],
        }
    }

    pub fn ids(self) -> Vec<TestId> {
        self.ids
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn add(&mut self, new_id: TestId) -> bool {
        // Halt if the new element is already contained
        for existing_id in &self.ids {
            if existing_id.contains(&new_id) {
                return false;
            }
        }

        // Remove any elements that are contained by the new element
        self.ids.retain(|existing_id| !new_id.contains(existing_id));

        // Add new element and sort list
        self.ids.push(new_id);
        self.ids.sort();

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adding_different_paths() {
        let sub1 = TestId::from("sub1");
        let sub2 = TestId::from("sub2");
        let sub3 = TestId::from("sub3");
        let root = TestId::root();

        let mut test_ids = TestIdCoverageSet::empty();

        assert_eq!(test_ids.len(), 0);
        assert_eq!(test_ids.add(sub1), true);
        assert_eq!(test_ids.add(sub2), true);
        assert_eq!(test_ids.add(sub3), true);
        assert_eq!(test_ids.len(), 3);
        assert_eq!(test_ids.add(root), true);
        assert_eq!(test_ids.len(), 1);
    }

    #[test]
    fn test_root_blocks_new_elements() {
        let root = TestId::root();
        let sub = TestId::from("sub");

        let mut test_ids = TestIdCoverageSet::empty();

        assert_eq!(test_ids.len(), 0);
        assert_eq!(test_ids.add(root), true);
        assert_eq!(test_ids.add(sub), false);
        assert_eq!(test_ids.len(), 1);
    }
}
