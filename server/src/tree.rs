use crate::constants::OYarn;

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Tree(pub Vec<OYarn>, pub Vec<OYarn>);

pub type TreeSlice<'a> = (&'a [OYarn], &'a [OYarn]);
pub type TreeStrSlice<'a> = (&'a [&'a str], &'a [&'a str]);

impl Tree {
    pub fn as_slice(&self) -> TreeSlice<'_> {
        (&self.0, &self.1)
    }

    pub fn flatten(mut self) -> Vec<OYarn> {
        self.0.extend(self.1);
        self.0
    }
}

/// Builds a Tree from a tuple of &'static str slices, e.g. `Tree::from((&["a"], &["b"]))`
impl From<TreeStrSlice<'static>> for Tree {
    fn from(tree: TreeStrSlice<'static>) -> Self {
        Tree(tree.0.iter().copied().map(OYarn::from).collect(),
             tree.1.iter().copied().map(OYarn::from).collect())
    }
}

/// Allows comparing a Tree to a pair of &str slices
impl PartialEq<TreeStrSlice<'_>> for Tree {
    fn eq(&self, other: &TreeStrSlice<'_>) -> bool {
        self.0 == other.0 && self.1 == other.1
    }
}

/// Allows comparing a Tree to a pair of &str arrays of any size, e.g. `(&["a"], &["b"])`
// In certain cases (&["a"], &["b"]) gets inferred as type (&[&str; 1], &[&str; 1])
// (tuple of references to arrays) instead of (&[&str], &[&str]) (tuple of slices)
// This impl allows the same ergonomics and the one above: `some_tree == (&["a"], &["b"])`
impl<const N: usize, const M: usize> PartialEq<(&[&str; N], &[&str; M])> for Tree {
    fn eq(&self, other: &(&[&str; N], &[&str; M])) -> bool {
        self.0 == *other.0 && self.1 == *other.1
    }
}


pub trait OYarnExt {
    fn starts_with_strs(&self, prefix: &[&str]) -> bool;
    fn ends_with_strs(&self, suffix: &[&str]) -> bool;
}

impl OYarnExt for [OYarn] {
    /// Equivalent of `&[OYarn].starts_with` that accepts `&[&str]` as the prefix
    fn starts_with_strs(&self, prefix: &[&str]) -> bool {
        self.len() >= prefix.len() && &self[..prefix.len()] == prefix
    }
    /// Equivalent of `&[OYarn].ends_with` that accepts `&[&str]` as the suffix
    fn ends_with_strs(&self, suffix: &[&str]) -> bool {
        self.len() >= suffix.len() && &self[self.len() - suffix.len()..] == suffix
    }
}
