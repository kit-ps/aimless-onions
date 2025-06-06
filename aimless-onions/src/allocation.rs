use std::{
    collections::{BinaryHeap, HashMap},
    mem,
};

use crate::{apitypes::RelayKey, nodename::NodeName};

// u32::MAX + 1
const WEIGHT_SPACE_SIZE: u128 = 1 << super::nodename::HIERARCHY_DEPTH;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocationRequest {
    pub id: u32,
    // Only used to provide stable sorting
    pub key: RelayKey,
    pub weight: u64,
}

impl PartialOrd for AllocationRequest {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        (self.weight, &self.key).partial_cmp(&(other.weight, &other.key))
    }
}

impl Ord for AllocationRequest {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.weight.cmp(&other.weight)
    }
}

#[derive(Debug, Clone)]
pub struct Allocation {
    pub id: u32,
    pub nodes: Vec<NodeName>,
}

pub fn allocate(requests: &[AllocationRequest]) -> Vec<Allocation> {
    let mut requests = Vec::from(requests);
    requests.sort_by_key(|r| r.key);
    let sum_weight: u64 = requests.iter().map(|r| r.weight).sum();
    for request in &mut requests {
        request.weight = (WEIGHT_SPACE_SIZE * u128::from(request.weight) / u128::from(sum_weight))
            .try_into()
            .expect("Weight was too large");
    }

    let mut allocations = requests
        .iter()
        .map(|r| {
            (
                r.id,
                Allocation {
                    id: r.id,
                    nodes: vec![],
                },
            )
        })
        .collect::<HashMap<_, _>>();
    let mut requests = BinaryHeap::from(requests);
    let mut free_cells = vec![NodeName::ROOT];
    let mut cell_size = NodeName::ROOT.subtree_size();

    loop {
        let Some(mut largest) = requests.pop() else {
            break;
        };

        if largest.weight == 0 {
            break;
        }

        if u128::from(largest.weight) < cell_size {
            cell_size /= 2;
            for cell in mem::take(&mut free_cells) {
                free_cells.push(cell.left());
                free_cells.push(cell.right());
            }
        } else {
            largest.weight -= u64::try_from(cell_size).unwrap();
            allocations.get_mut(&largest.id).unwrap().nodes.push(
                free_cells
                    .pop()
                    .expect("We expected to have more free cells"),
            );
        }
        requests.push(largest);
    }

    allocations.into_values().collect()
}
