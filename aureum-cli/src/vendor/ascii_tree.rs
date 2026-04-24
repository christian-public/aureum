// Source code copied from the crates.io package: ascii_tree v0.1.1
// Original author: d.maetzke@bpressure.net
// License: MIT

//! Write an ascii tree

use colored::Colorize;
use std::fmt;
use std::fmt::Write;

#[derive(Clone)]
pub enum Tree {
    Node(String, Vec<Tree>),
    Leaf(Vec<String>),
}

#[inline]
/// writes a tree in an ascii tree to the writer
pub fn write_tree(f: &mut dyn Write, tree: &Tree) -> fmt::Result {
    write_tree_element(f, tree, &[])
}

fn write_tree_element(f: &mut dyn Write, tree: &Tree, level: &[usize]) -> fmt::Result {
    use Tree::*;
    const EMPTY: &str = "    ";
    const EDGE: &str = "└── ";
    const PIPE: &str = "│   ";
    const BRANCH: &str = "├── ";

    // NOTE: The code compiles without `.to_string()` but the `PIPE` string is not colored correctly.
    let colored = |text: &str| text.dimmed().to_string();

    let maxpos = level.len();
    let mut second_line = String::new();
    for (pos, l) in level.iter().enumerate() {
        let last_row = pos == maxpos - 1;
        if *l == 1 {
            if !last_row {
                write!(f, "{}", colored(EMPTY))?
            } else {
                write!(f, "{}", colored(EDGE))?
            }
            second_line.push_str(&colored(EMPTY));
        } else {
            if !last_row {
                write!(f, "{}", colored(PIPE))?
            } else {
                write!(f, "{}", colored(BRANCH))?
            }
            second_line.push_str(&colored(PIPE));
        }
    }

    match tree {
        Node(title, children) => {
            let mut d = children.len();
            writeln!(f, "{}", title)?;
            for s in children {
                let mut lnext = level.to_owned();
                lnext.push(d);
                d -= 1;
                write_tree_element(f, s, &lnext)?;
            }
        }
        Leaf(lines) => {
            for (i, s) in lines.iter().enumerate() {
                match i {
                    0 => writeln!(f, "{}", s)?,
                    _ => writeln!(f, "{}{}", second_line, s)?,
                }
            }
        }
    }

    Ok(())
}
