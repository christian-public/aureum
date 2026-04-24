pub mod ascii_tree;

use std::fmt;

impl fmt::Display for ascii_tree::Tree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        ascii_tree::write_tree(f, self)
    }
}
