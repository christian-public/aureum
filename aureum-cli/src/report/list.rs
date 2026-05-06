use crate::vendor::ascii_tree::Tree::{self, Leaf, Node};
use aureum::TestId;
use relative_path::RelativePathBuf;
use std::collections::BTreeMap;

pub fn print_test_list_as_tree(items: &[(RelativePathBuf, &TestId)]) {
    let tree = build_test_list_tree(items);
    let output = tree.to_string();

    print!("{}", output);
}

fn build_test_list_tree(items: &[(RelativePathBuf, &TestId)]) -> Tree {
    let mut by_file: BTreeMap<Vec<String>, Vec<String>> = BTreeMap::new();

    for (file_path, test_id) in items {
        let segments: Vec<String> = file_path
            .components()
            .map(|c| c.as_str().to_string())
            .collect();
        let subtests = by_file.entry(segments).or_default();
        if !test_id.is_root() {
            subtests.push(format!(":{}", test_id));
        }
    }

    build_tree_node("/", &by_file, &[])
}

fn build_tree_node(
    label: &str,
    entries: &BTreeMap<Vec<String>, Vec<String>>,
    prefix: &[String],
) -> Tree {
    let mut children: BTreeMap<String, Tree> = BTreeMap::new();

    for (segments, subtests) in entries {
        if !segments.starts_with(prefix) {
            continue;
        }
        match &segments[prefix.len()..] {
            [file] => {
                let child = if subtests.is_empty() {
                    Leaf(vec![file.clone()])
                } else {
                    let leaves = subtests.iter().map(|s| Leaf(vec![s.clone()])).collect();
                    Node(file.clone(), leaves)
                };
                children.insert(file.clone(), child);
            }
            [dir, ..] => {
                if !children.contains_key(dir.as_str()) {
                    let mut child_prefix = prefix.to_vec();
                    child_prefix.push(dir.clone());
                    children.insert(
                        dir.clone(),
                        build_tree_node(&format!("{dir}/"), entries, &child_prefix),
                    );
                }
            }
            [] => {}
        }
    }

    Node(label.to_string(), children.into_values().collect())
}
