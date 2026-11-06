//! Files as a tree of folders, for a review's file list and the file tree
//! tab. A folder holding nothing but one other folder shares its line
//! (`src/review/`), so deep paths do not cost a line per level. Files keep
//! the order they came in, which keeps each folder's files together, so a
//! review's tree read top to bottom is its diff's order too.
use crate::review::diff::Diff;
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Node {
    Folder {
        /// The folder's full path, which keys its folding.
        path: String,
        /// What its line shows: its own name, or a chain of them.
        label: String,
        depth: usize,
    },
    File {
        file: usize,
        depth: usize,
    },
}

impl Node {
    pub(crate) fn depth(&self) -> usize {
        match self {
            Self::Folder { depth, .. } | Self::File { depth, .. } => *depth,
        }
    }
}

/// A folder while the tree is built: its sub-folders and files in order.
#[derive(Default)]
struct Dir {
    name: String,
    children: Vec<Child>,
}

enum Child {
    Dir(Dir),
    File(usize),
}

impl Dir {
    fn insert(&mut self, parts: &[&str], file: usize) {
        let [name, rest @ ..] = parts else {
            return;
        };
        if rest.is_empty() {
            self.children.push(Child::File(file));
            return;
        }
        // A folder's files arrive together.
        match self.children.last_mut() {
            Some(Child::Dir(dir)) if dir.name == *name => dir.insert(rest, file),
            _ => {
                let mut dir = Dir {
                    name: (*name).to_owned(),
                    children: Vec::new(),
                };
                dir.insert(rest, file);
                self.children.push(Child::Dir(dir));
            }
        }
    }

    fn flatten(&self, prefix: &str, depth: usize, nodes: &mut Vec<Node>) {
        for child in &self.children {
            match child {
                Child::File(file) => nodes.push(Node::File { file: *file, depth }),
                Child::Dir(dir) => {
                    // A folder with one sub-folder and nothing else joins it.
                    let mut label = dir.name.clone();
                    let mut inner = dir;
                    while let [Child::Dir(only)] = inner.children.as_slice() {
                        label = format!("{label}/{}", only.name);
                        inner = only;
                    }
                    let path = format!("{prefix}{label}/");
                    nodes.push(Node::Folder {
                        path: path.clone(),
                        label: format!("{label}/"),
                        depth,
                    });
                    inner.flatten(&path, depth + 1, nodes);
                }
            }
        }
    }
}

/// Every folder and file, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Tree {
    pub nodes: Vec<Node>,
}

impl Tree {
    pub(crate) fn build(diff: &Diff) -> Self {
        Self::of(diff.files.iter().map(|file| file.path.as_str()))
    }

    /// The tree of `paths`, a node's `file` being its place among them.
    /// Each folder's paths must come together.
    pub(crate) fn of<'a>(paths: impl IntoIterator<Item = &'a str>) -> Self {
        let mut root = Dir::default();
        let mut count = 0;
        for (index, path) in paths.into_iter().enumerate() {
            let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
            root.insert(&parts, index);
            count += 1;
        }
        let mut nodes = Vec::with_capacity(count);
        root.flatten("", 0, &mut nodes);
        Self { nodes }
    }

    /// The nodes the list shows: the files `keep` keeps, under their
    /// folders, and nothing inside a `closed` folder but the folder. A
    /// folder with no kept file shows nothing. `filtered` opens every
    /// folder, so a filter's matches all show.
    pub(crate) fn shown(
        &self,
        closed: &HashSet<String>,
        filtered: bool,
        keep: impl Fn(usize) -> bool,
    ) -> Vec<usize> {
        let mut shown = Vec::new();
        // The folders above the next node: each one's node, whether it is
        // shown yet, and whether it is closed.
        let mut stack: Vec<(usize, bool, bool)> = Vec::new();
        for (index, node) in self.nodes.iter().enumerate() {
            stack.truncate(node.depth());
            match node {
                Node::Folder { path, .. } => {
                    stack.push((index, false, !filtered && closed.contains(path)));
                }
                Node::File { file, .. } => {
                    if !keep(*file) {
                        continue;
                    }
                    let mut open = true;
                    for (folder, seen, closed) in &mut stack {
                        if !*seen {
                            shown.push(*folder);
                            *seen = true;
                        }
                        if *closed {
                            open = false;
                            break;
                        }
                    }
                    if open {
                        shown.push(index);
                    }
                }
            }
        }
        shown
    }
}

#[cfg(test)]
mod tests;
