use crate::utils::tree;
use crate::vendor::ascii_tree::Tree::{self, Leaf, Node};
use aureum::TestCase;
use std::collections::BTreeMap;

pub fn print_test_list_as_tree(test_cases: &[TestCase]) {
    let tree = build_test_list_tree(test_cases);
    let output = tree::draw_tree(&tree).unwrap_or(String::from("Failed to draw tree\n"));

    print!("{}", output);
}

fn build_test_list_tree(test_cases: &[TestCase]) -> Tree {
    let mut by_file: BTreeMap<Vec<String>, Vec<String>> = BTreeMap::new();

    for test_case in test_cases {
        let segments: Vec<String> = test_case
            .path_to_config_file()
            .components()
            .map(|c| c.as_str().to_string())
            .collect();
        let subtests = by_file.entry(segments).or_default();
        if !test_case.test_id.is_root() {
            subtests.push(format!(":{}", test_case.test_id));
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
