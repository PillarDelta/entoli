use crate::value::Value;
use std::rc::Rc;

/// Persistent environment: an immutable linked list of bindings. Cheap to extend,
/// structurally shared, and walkable by the encoder.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Env(Option<Rc<EnvNode>>);

#[derive(Debug, PartialEq)]
pub struct EnvNode {
    pub name: String,
    pub value: Value,
    pub next: Env,
}

impl Env {
    pub fn empty() -> Self {
        Env(None)
    }

    /// The head node, for structure-aware consumers (the encoder's sharing
    /// table keys on its Rc identity).
    pub fn node(&self) -> Option<&Rc<EnvNode>> {
        self.0.as_ref()
    }

    pub fn from_node(node: Option<Rc<EnvNode>>) -> Env {
        Env(node)
    }

    pub fn bind(&self, name: impl Into<String>, value: Value) -> Env {
        Env(Some(Rc::new(EnvNode { name: name.into(), value, next: self.clone() })))
    }

    pub fn lookup(&self, name: &str) -> Option<&Value> {
        let mut cur = self;
        while let Env(Some(node)) = cur {
            if node.name == name {
                return Some(&node.value);
            }
            cur = &node.next;
        }
        None
    }

    /// Iterate bindings innermost-first (shadowing order), for the encoder.
    pub fn iter(&self) -> EnvIter<'_> {
        EnvIter { cur: self }
    }
}

pub struct EnvIter<'a> {
    cur: &'a Env,
}

impl<'a> Iterator for EnvIter<'a> {
    type Item = (&'a str, &'a Value);
    fn next(&mut self) -> Option<Self::Item> {
        match self.cur {
            Env(Some(node)) => {
                self.cur = &node.next;
                Some((node.name.as_str(), &node.value))
            }
            Env(None) => None,
        }
    }
}
