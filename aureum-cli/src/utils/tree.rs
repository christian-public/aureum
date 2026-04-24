use crate::vendor::ascii_tree;
use crate::vendor::ascii_tree::Tree;
use std::fmt::Error;

pub fn draw_tree(tree: &Tree) -> Result<String, Error> {
    let mut output = String::new();
    ascii_tree::write_tree(&mut output, tree)?;
    Ok(output)
}
