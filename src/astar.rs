//! Compute a shortest path (or all shorted paths) using the [A* search
//! algorithm](https://en.wikipedia.org/wiki/A*_search_algorithm).

use actix_web::web::Data;
use indexmap::map::Entry::{Occupied, Vacant};
use indexmap::IndexMap;
use num_traits::Zero;
use rustc_hash::FxHasher;
use std::cmp::Ordering;
use std::collections::{BinaryHeap};
use std::hash::{BuildHasherDefault, Hash};
use std::sync::atomic::{AtomicBool};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::usize;

use crate::AppState;
use crate::data::node::Node;

type FxIndexMap<K, V> = IndexMap<K, V, BuildHasherDefault<FxHasher>>;

/// Compute a shortest path using the [A* search
/// algorithm](https://en.wikipedia.org/wiki/A*_search_algorithm).
///
/// The shortest path starting from `start` up to a node for which `success` returns `true` is
/// computed and returned along with its total cost, in a `Some`. If no path can be found, `None`
/// is returned instead.
///
/// - `start` is the starting node.
/// - `successors` returns a list of successors for a given node, along with the cost for moving
/// from the node to the successor. This cost must be non-negative.
/// - `heuristic` returns an approximation of the cost from a given node to the goal. The
/// approximation must not be greater than the real cost, or a wrong shortest path may be returned.
/// - `success` checks whether the goal has been reached. It is not a node as some problems require
/// a dynamic solution instead of a fixed node.
///
/// A node will never be included twice in the path as determined by the `Eq` relationship.
///
/// The returned path comprises both the start and end node.
///
/// # Example
///
/// We will search the shortest path on a chess board to go from (1, 1) to (4, 6) doing only knight
/// moves.
///
/// The first version uses an explicit type `Pos` on which the required traits are derived.
///
/// ```
/// use pathfinding::prelude::astar;
///
/// #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
/// struct Pos(i32, i32);
///
/// impl Pos {
///   fn distance(&self, other: &Pos) -> u32 {
///     (self.0.abs_diff(other.0) + self.1.abs_diff(other.1)) as u32
///   }
///
///   fn successors(&self) -> Vec<(Pos, u32)> {
///     let &Pos(x, y) = self;
///     vec![Pos(x+1,y+2), Pos(x+1,y-2), Pos(x-1,y+2), Pos(x-1,y-2),
///          Pos(x+2,y+1), Pos(x+2,y-1), Pos(x-2,y+1), Pos(x-2,y-1)]
///          .into_iter().map(|p| (p, 1)).collect()
///   }
/// }
///
/// static GOAL: Pos = Pos(4, 6);
/// let result = astar(&Pos(1, 1), |p| p.successors(), |p| p.distance(&GOAL) / 3,
///                    |p| *p == GOAL);
/// assert_eq!(result.expect("no path found").1, 4);
/// ```
///
/// The second version does not declare a `Pos` type, makes use of more closures,
/// and is thus shorter.
///
/// ```
/// use pathfinding::prelude::astar;
///
/// static GOAL: (i32, i32) = (4, 6);
/// let result = astar(&(1, 1),
///                    |&(x, y)| vec![(x+1,y+2), (x+1,y-2), (x-1,y+2), (x-1,y-2),
///                                   (x+2,y+1), (x+2,y-1), (x-2,y+1), (x-2,y-1)]
///                               .into_iter().map(|p| (p, 1)),
///                    |&(x, y)| (GOAL.0.abs_diff(x) + GOAL.1.abs_diff(y)) / 3,
///                    |&p| p == GOAL);
/// assert_eq!(result.expect("no path found").1, 4);
/// ```
#[allow(clippy::missing_panics_doc)]
pub fn astar(
    start: Node,
    end: Node,
    state: Data<AppState>
) -> Option<(Vec<Node>, i64)>
{
    let to_see = Arc::new(Mutex::new(BinaryHeap::new()));
    to_see.lock().unwrap().push(SmallestCostHolder {
        estimated_cost: Zero::zero(),
        cost: Zero::zero(),
        index: 0,
    });
    let parents = Arc::new(Mutex::new(FxIndexMap::default()));
    parents
        .lock()
        .unwrap()
        .insert(start.clone(), (usize::max_value(), Zero::zero()));
    let pool = Arc::new(rayon::ThreadPoolBuilder::new()
        .num_threads(22)
        .build()
        .unwrap());
    let (tx_to_see_push, rx_to_see_push) = channel();
    let (tx_success, rx_success) = channel();
    let done = Arc::new(AtomicBool::new(false));
    pool.clone().spawn(move || {
        loop {
            let mut guard = to_see.lock().unwrap();
            let (cost, index) = match guard.pop() {
                Some(SmallestCostHolder { cost, index, .. }) => (cost, index),
                None => {
                    drop(guard);
                    rx_to_see_push.recv().unwrap();
                    continue;
                }
            };
            drop(guard);
            let to_see = to_see.clone();
            let parents = parents.clone();
            let tx_to_see_push = tx_to_see_push.clone();
            let done = done.clone();
            let tx_success = tx_success.clone();
            let end = end.clone();
            let state = state.clone();
            pool.spawn(move || {
                if done.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                let successors = {
                    let guard = parents.lock().unwrap();
                    let guard2 = guard.clone();
                    let (node, &(_, c)) = guard2.get_index(index).unwrap(); // Cannot fail
                    if node.lat == end.lat && node.lon == end.lon {
                        let path = reverse_path(guard2.clone(), |&(p, _)| p, index);
                        done.store(true, std::sync::atomic::Ordering::Relaxed);
                        tx_success.send(Some((path, cost))).unwrap_or_else(|_|{return;});
                        return;
                    }
                    drop(guard);
                    // We may have inserted a node several time into the binary heap if we found
                    // a better way to access it. Ensure that we are currently dealing with the
                    // best path and discard the others.
                    if cost > c {
                        // tx.send(None).unwrap();
                        return;
                    }
                    node.successors(state).unwrap()
                };
                for (successor, move_cost) in successors {
                    let new_cost = cost as i64 + move_cost;
                    let h; // heuristic(&successor)
                    let n; // index for successor
                    match parents.lock().unwrap().entry(successor.clone()) {
                        Vacant(e) => {
                            h = successor.distance(&end);
                            n = e.index();
                            e.insert((index, new_cost));
                        }
                        Occupied(mut e) => {
                            if e.get().1 > new_cost {
                                h = successor.distance(e.key());
                                n = e.index();
                                e.insert((index, new_cost));
                            } else {
                                continue;
                            }
                        }
                    }
                    let mut to_see = to_see.lock().unwrap();
                    to_see.push(SmallestCostHolder {
                        estimated_cost: new_cost + h as i64,
                        cost: new_cost,
                        index: n,
                    });
                    tx_to_see_push.send(true).unwrap();
                }
            });
        }
    });
    let r = rx_success.recv().unwrap();
    println!("Got result");
    r
}

struct SmallestCostHolder<K> {
    estimated_cost: K,
    cost: K,
    index: usize,
}

impl<K: PartialEq> PartialEq for SmallestCostHolder<K> {
    fn eq(&self, other: &Self) -> bool {
        self.estimated_cost.eq(&other.estimated_cost) && self.cost.eq(&other.cost)
    }
}

impl<K: PartialEq> Eq for SmallestCostHolder<K> {}

impl<K: Ord> PartialOrd for SmallestCostHolder<K> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<K: Ord> Ord for SmallestCostHolder<K> {
    fn cmp(&self, other: &Self) -> Ordering {
        match other.estimated_cost.cmp(&self.estimated_cost) {
            Ordering::Equal => self.cost.cmp(&other.cost),
            s => s,
        }
    }
}

#[allow(clippy::needless_collect)]
fn reverse_path<N, V, F>(parents: FxIndexMap<N, V>, mut parent: F, start: usize) -> Vec<N>
where
    N: Eq + Hash + Clone,
    F: FnMut(&V) -> usize,
{
    let mut i = start;
    let path = std::iter::from_fn(|| {
        parents.get_index(i).map(|(node, value)| {
            i = parent(value);
            node
        })
    })
    .collect::<Vec<&N>>();
    // Collecting the going through the vector is needed to revert the path because the
    // unfold iterator is not double-ended due to its iterative nature.
    path.into_iter().rev().cloned().collect()
}
