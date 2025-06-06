// The node name representation was chosen to be in line with the notation used in the paper:
// In the paper 'w0' and 'w1' are the children of the node 'w', here we have 'self.1 << 1' and
// '(self.1 << 1) | 1'. This makes it easier to follow along.
use arrayvec::ArrayVec;

use hohibe::{
    hibe::{BonehBoyenGoh, Hibe},
    kem::HashMapper,
    Mapper,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeName(u8, u64);

pub const HIERARCHY_DEPTH: u8 = 32;

impl NodeName {
    pub const ROOT: Self = NodeName(0, 0);
    pub const MAX: Self = NodeName(HIERARCHY_DEPTH, u64::MAX);

    fn canonicalized(mut self) -> NodeName {
        let mut mask = u64::MAX;
        for i in self.0..64 {
            mask ^= 1 << i;
        }
        self.1 &= mask;
        self
    }

    pub fn number(path: u64) -> Self {
        NodeName(HIERARCHY_DEPTH, path).canonicalized()
    }

    pub fn new(length: u8, path: u64) -> Self {
        assert!(length <= HIERARCHY_DEPTH);
        NodeName(length, path).canonicalized()
    }

    pub fn parent(self) -> NodeName {
        assert!(self.0 > 0);
        NodeName(self.0 - 1, self.1 >> 1)
    }

    pub fn left(self) -> NodeName {
        assert!(self.0 < HIERARCHY_DEPTH);
        NodeName(self.0 + 1, self.1 << 1)
    }

    pub fn right(self) -> NodeName {
        assert!(self.0 < HIERARCHY_DEPTH);
        NodeName(self.0 + 1, (self.1 << 1) | 1)
    }

    pub fn is_left(self) -> bool {
        self.1 & 1 == 0 && self.is_leaf()
    }

    pub fn is_right(self) -> bool {
        self.1 & 1 == 1 && self.is_leaf()
    }

    pub fn subtree_size(self) -> u128 {
        1 << (HIERARCHY_DEPTH - self.len())
    }

    pub fn len(self) -> u8 {
        self.0
    }

    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    pub fn path(self) -> u64 {
        self.1
    }

    pub fn contains(self, other: NodeName) -> bool {
        self.0 <= other.0 && (other.1 >> (other.0 - self.0)) == self.1
    }

    pub fn is_leaf(self) -> bool {
        self.0 == HIERARCHY_DEPTH
    }

    pub fn walk(mut self) -> impl Iterator<Item = NodeName> {
        let mut parents = ArrayVec::<NodeName, {HIERARCHY_DEPTH as usize}>::new();
        while !self.is_empty() {
            parents.push(self);
            self = self.parent();
        }
        parents.reverse();
        parents.into_iter()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodenameMapper;

impl NodenameMapper {
    pub fn identity_matrix() -> Vec<Vec<<BonehBoyenGoh as Hibe>::Identity>> {
        let a = NodeName::number(0x0000000000000000);
        let b = NodeName::number(0xFFFFFFFFFFFFFFFF);
        let id_a = NodenameMapper.map_identity(a).unwrap();
        let id_b = NodenameMapper.map_identity(b).unwrap();
        let mut result = Vec::new();
        for (elem_a, elem_b) in id_a.into_iter().zip(id_b.into_iter()) {
            result.push(vec![elem_a, elem_b])
        }
        result
    }
}

impl Mapper<NodeName, <BonehBoyenGoh as Hibe>::Identity> for NodenameMapper {
    fn map_identity(
        &self,
        input: NodeName,
    ) -> hohibe::error::Result<Vec<<BonehBoyenGoh as Hibe>::Identity>> {
        let path = (1..=input.len()).map(|i| input.path() >> (input.len() - i) & 1);
        HashMapper.map_identity(path)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn left_child() {
        assert_eq!(NodeName::ROOT.left(), NodeName::new(1, 0));
    }

    #[test]
    fn right_child() {
        assert_eq!(NodeName::ROOT.right(), NodeName::new(1, 1));
    }

    #[test]
    fn walk() {
        let node = NodeName::ROOT.left().right().right();
        let walk = node.walk().collect::<Vec<_>>();
        assert_eq!(
            walk,
            vec![
                NodeName::ROOT.left(),
                NodeName::ROOT.left().right(),
                NodeName::ROOT.left().right().right()
            ]
        );
    }

    #[test]
    fn contains() {
        assert!(NodeName::ROOT.contains(NodeName::ROOT));
        assert!(NodeName::ROOT.contains(NodeName::ROOT.left()));
        assert!(NodeName::ROOT.contains(NodeName::ROOT.right()));
        assert!(NodeName::ROOT.contains(NodeName::ROOT.right().left()));
        assert!(NodeName::ROOT.left().contains(NodeName::ROOT.left().left()));

        assert!(!NodeName::ROOT.left().contains(NodeName::ROOT));
        assert!(!NodeName::ROOT.left().contains(NodeName::ROOT.right()));
    }
}
