use std::{collections::HashMap, str::Chars};

#[derive(Debug, Clone)]
pub struct Node<T> {
    prefix: String,
    children: HashMap<char, Node<T>>,
    value: Option<T>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeRef<'a, T> {
    pub prefix: &'a str,
    pub value: &'a T,
}

impl<T> Node<T> {
    fn insert(&mut self, mut chars: Chars, value: T, prefix: &str) -> Option<T> {
        match chars.next() {
            Some(char) => {
                let prefix = format!("{prefix}{char}");
                let child = self.children.entry(char).or_insert_with(|| Node {
                    prefix: prefix.clone(),
                    children: HashMap::default(),
                    value: None,
                });
                child.insert(chars, value, &prefix)
            }
            None => self.value.replace(value),
        }
    }

    fn remove(&mut self, mut chars: Chars) -> bool {
        if let Some(char) = chars.next() {
            let Some(child) = self.children.get_mut(&char) else {
                return false;
            };
            if child.remove(chars) {
                self.children.remove(&char);
                self.children.is_empty()
            } else {
                false
            }
        } else {
            self.value = None;
            self.children.is_empty()
        }
    }

    fn longest_prefix<'a>(
        &'a self,
        mut chars: Chars,
        current: Option<&'a Node<T>>,
    ) -> Option<NodeRef<'a, T>> {
        let new_current = self.value.as_ref().map(|_| self).or(current);
        match chars.next() {
            Some(char) => match self.children.get(&char) {
                Some(c) => c.longest_prefix(chars, new_current),
                None => new_current.and_then(|n| {
                    n.value.as_ref().map(|val| NodeRef {
                        prefix: &n.prefix,
                        value: val,
                    })
                }),
            },
            None => new_current.and_then(|n| {
                n.value.as_ref().map(|val| NodeRef {
                    prefix: &n.prefix,
                    value: val,
                })
            }),
        }
    }

    fn get(&self, mut chars: Chars) -> Option<&Self> {
        match chars.next() {
            Some(char) => {
                let child = self.children.get(&char)?;
                child.get(chars)
            }
            None => Some(self),
        }
    }

    fn mounts(&self) -> Vec<NodeRef<'_, T>> {
        self.children
            .values()
            .flat_map(|node| {
                if let Some(value) = node.value.as_ref() {
                    let mut mounts = node.mounts();
                    mounts.push(NodeRef {
                        prefix: &node.prefix,
                        value,
                    });
                    mounts
                } else {
                    node.mounts()
                }
            })
            .collect()
    }
}

/// A very naive implementation of a trie (not even radix trie).
///
/// This can be optimized in all sorts of ways if found to be a bottleneck in
/// the routing which is its main use-case.
#[derive(Debug, Clone)]
pub struct Trie<T> {
    pub(crate) root: HashMap<char, Node<T>>,
}

impl<T> Default for Trie<T> {
    fn default() -> Self {
        Self {
            root: HashMap::default(),
        }
    }
}

impl<T> Trie<T> {
    pub fn insert(&mut self, path: &str, value: T) -> Option<T> {
        let mut chars = path.chars();
        let first_char = chars.next()?;
        let root = self.root.entry(first_char).or_insert_with(|| Node {
            prefix: first_char.to_string(),
            children: HashMap::default(),
            value: None,
        });

        root.insert(chars, value, &first_char.to_string())
    }

    pub fn clear(&mut self) {
        self.root = HashMap::default();
    }

    #[allow(unused)]
    pub fn remove(&mut self, path: &str) -> bool {
        let mut chars = path.chars();
        let Some(first_char) = chars.next() else {
            return false;
        };
        let Some(root) = self.root.get_mut(&first_char) else {
            return false;
        };
        if root.remove(chars) {
            self.root.remove(&first_char).is_some()
        } else {
            false
        }
    }

    pub fn longest_prefix(&self, path: &str) -> Option<NodeRef<'_, T>> {
        let mut chars = path.chars();
        let first_char = chars.next()?;
        let root = self.root.get(&first_char)?;
        root.longest_prefix(chars, None)
    }

    pub fn get(&self, path: &str) -> Option<&T> {
        let mut chars = path.chars();
        let first_char = chars.next()?;
        let root = self.root.get(&first_char)?;
        root.get(chars).and_then(|node| node.value.as_ref())
    }

    pub fn mounts(&self) -> Vec<NodeRef<'_, T>> {
        let mut mounts: Vec<NodeRef<'_, T>> = self.root.values().flat_map(Node::mounts).collect();
        mounts.sort_by_key(|node| node.prefix);
        mounts
    }
}

#[cfg(test)]
mod tests {
    use crate::helpers::trie::NodeRef;

    use super::Trie;

    #[test]
    fn insert_and_get() {
        let mut trie = Trie::default();
        assert_eq!(trie.get("/foo"), None);
        assert!(trie.insert("/foo", "bar").is_none());
        assert_eq!(trie.get("/foo"), Some(&"bar"));
    }

    #[test]
    fn remove() {
        let mut trie = Trie::default();
        assert!(trie.insert("/foo", "bar").is_none());
        assert_eq!(trie.get("/foo"), Some(&"bar"));
        trie.remove("/foo");
        assert_eq!(trie.get("/foo"), None);
    }

    #[test]
    fn get() {
        let mut trie = Trie::default();
        assert!(trie.insert("/foo", "foo").is_none());
        assert!(trie.insert("/foo/bar", "foo_bar").is_none());
        assert!(trie.insert("/foo/bar/baz", "foo_bar_baz").is_none());
        assert_eq!(trie.get("/foo"), Some(&"foo"));
        assert_eq!(trie.get("/foo/"), None);
        assert_eq!(trie.get("/foo/bar"), Some(&"foo_bar"));
        assert_eq!(trie.get("/foo/bar/"), None);
        assert_eq!(trie.get("/foo/bar/baz"), Some(&"foo_bar_baz"));
        assert_eq!(trie.get("/foo/bar/baz/"), None);
    }

    #[test]
    fn longest_prefix() {
        let mut trie = Trie::default();
        assert!(trie.insert("/foo", "foo").is_none());
        assert!(trie.insert("/foo/bar", "foo_bar").is_none());
        assert!(trie.insert("/foo/bar/baz", "foo_bar_baz").is_none());
        assert_eq!(trie.longest_prefix("/"), None);
        assert_eq!(
            trie.longest_prefix("/foo"),
            Some(NodeRef {
                prefix: &"/foo",
                value: &"foo",
            })
        );
        assert_eq!(
            trie.longest_prefix("/foo/ba"),
            Some(NodeRef {
                prefix: &"/foo",
                value: &"foo",
            })
        );
        assert_eq!(
            trie.longest_prefix("/foo/bar"),
            Some(NodeRef {
                prefix: &"/foo/bar",
                value: &"foo_bar",
            })
        );
        assert_eq!(
            trie.longest_prefix("/foo/bar/ba"),
            Some(NodeRef {
                prefix: &"/foo/bar",
                value: &"foo_bar",
            })
        );
        assert_eq!(
            trie.longest_prefix("/foo/bar/baz"),
            Some(NodeRef {
                prefix: &"/foo/bar/baz",
                value: &"foo_bar_baz",
            })
        );
        assert_eq!(
            trie.longest_prefix("/foo/bar/baz/"),
            Some(NodeRef {
                prefix: &"/foo/bar/baz",
                value: &"foo_bar_baz",
            })
        );
    }

    #[test]
    fn get_router_mounts() {
        let mut trie = Trie::default();
        assert!(trie.insert("/foo", "foo").is_none());
        assert!(trie.insert("/foo/bar", "foo").is_none());
        assert!(trie.insert("/foo/bar2", "foo").is_none());
        assert!(trie.insert("/foo/bar3", "foo").is_none());
        assert!(trie.insert("/foo/bar4/baz", "foo").is_none());

        assert_eq!(
            trie.mounts(),
            vec![
                NodeRef {
                    prefix: &"/foo",
                    value: &"foo"
                },
                NodeRef {
                    prefix: &"/foo/bar",
                    value: &"foo"
                },
                NodeRef {
                    prefix: &"/foo/bar2",
                    value: &"foo"
                },
                NodeRef {
                    prefix: &"/foo/bar3",
                    value: &"foo"
                },
                NodeRef {
                    prefix: &"/foo/bar4/baz",
                    value: &"foo"
                },
            ]
        );
    }
}
