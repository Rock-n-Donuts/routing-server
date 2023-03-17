//! Compute a shortest path (or all shorted paths) using the [A* search
//! algorithm](https://en.wikipedia.org/wiki/A*_search_algorithm).

use actix_web::web::Data;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex, RwLock};
use crate::data::node::Node;
use crate::AppState;

pub fn astar(start: Node, end: Node, state: Data<AppState>) -> Option<(Vec<Node>, i64)> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(22)
        .build()
        .unwrap();
    let response: Arc<RwLock<Option<Arc<RwLock<SearchSate>>>>> = Arc::new(RwLock::new(None));
    let start = start.clone();
    let response_clone = response.clone();
    let state_clone = state.clone();
    let node_id_search_state_map = Arc::new(Mutex::new(HashMap::new()));
    let node_id_search_state_map_clone = node_id_search_state_map.clone();
    pool.scope(move |scope| {
        let node_id_search_state_map = node_id_search_state_map_clone;
        let state = state_clone;
        let response = response_clone;
        let (tx_to_see_push, rx_to_see_push) = channel();
        let to_see = Arc::new(RwLock::new(Vec::new()));
        let search_state = Arc::new(RwLock::new(SearchSate {
            cost: 0,
            node: start.clone(),
            parent_id: None,
        }));
        node_id_search_state_map
            .lock()
            .unwrap()
            .insert(start.id, search_state.clone());
        to_see.write().unwrap().push(search_state.clone());
        loop {
            // We already have the response
            if response.read().unwrap().is_some() {
                return;
            }
            let mut a_voir = "".to_string();
            to_see.read().unwrap().iter().for_each(|state| {
                a_voir = a_voir.to_owned() + &state.read().unwrap().node.distance(&end).to_string() + "-" + &state.read().unwrap().cost.to_string() +  ", ";
            });
            println!("a voir: {}", a_voir);
            let mut locked_to_see = to_see.write().unwrap();
            let search_state = match locked_to_see.pop() {
                Some(search_state) => search_state,
                None => {
                    drop(locked_to_see);
                    rx_to_see_push.recv().unwrap();
                    continue;
                }
            };
            drop(locked_to_see);
            println!("distance: {}", search_state.read().unwrap().node.distance(&end));
            let tx_to_see_push = tx_to_see_push.clone();
            let end = end.clone();
            let search_state = search_state.clone();
            let response = response.clone();
            let to_see = to_see.clone();
            let state = state.clone();
            let node_id_search_state_map = node_id_search_state_map.clone();
            scope.spawn(move |_scope| {
                if response.read().unwrap().is_some() {
                    return;
                }
                let state_locked = search_state.read().unwrap();
                if state_locked.node.lat == end.lat && state_locked.node.lon == end.lon {
                    response.write().unwrap().replace(search_state.clone());
                    return;
                }
                let successors = state_locked.node.successors(state).unwrap();
                for (successor, move_cost) in successors {
                    let new_cost = state_locked.cost + move_cost;
                    let h = successor.distance(&end); // heuristic(&successor)
                    let mut to_see = to_see.write().unwrap();
                    let mut node_id_search_state_map = node_id_search_state_map.lock().unwrap();
                    match node_id_search_state_map.get(&successor.id) {
                        Some(search_state) => {
                            if search_state.read().unwrap().cost <= new_cost + h as i64 {
                                // We already have a better path to this node
                                continue;
                            }
                            let new_state = Arc::new(RwLock::new(SearchSate {
                                cost: new_cost + h as i64,
                                node: successor.clone(),
                                parent_id: Some(state_locked.node.id),
                            }));
                            // Replace the old state with the new one
                            node_id_search_state_map.insert(successor.id, new_state.clone());
                            // Remove the old state from the to_see list
                            to_see.retain(|state| state.read().unwrap().node.id != successor.id);
                            // Add the new state to the to_see list
                            to_see.push(new_state);
                        }
                        None => {
                            let new_state = Arc::new(RwLock::new(SearchSate {
                                cost: new_cost + h as i64,
                                node: successor.clone(),
                                parent_id: Some(state_locked.node.id),
                            }));
                            node_id_search_state_map.insert(successor.id, new_state.clone());
                            to_see.push(new_state);
                        }
                    }
                    to_see.sort_by(|b, a| a.read().unwrap().cost.cmp(&b.read().unwrap().cost));
                    tx_to_see_push.send(true).unwrap_or_else(|e| {
                        println!("Failed to end to to_see_push {:?}", e);
                        return;
                    });
                }
            });
        }
    });

    // Prepare response
    let mut transformed_response = Vec::new();
    println!("transforming response...");
    loop {
        println!("looping...");
        let mut locked_response = response.write().unwrap();
        match locked_response.clone() {
            Some(search_state) => {
                let locked_search_state = search_state.read().unwrap();
                transformed_response.push(locked_search_state.node.clone());
                println!("pushed");
                match locked_search_state.parent_id {
                    Some(pid) => {
                        println!("parent_id: {:?}", locked_search_state.parent_id);
                        let locked_node_id_search_state_map = node_id_search_state_map.lock().unwrap();
                        let next_search_state = locked_node_id_search_state_map
                            .get(&pid)
                            .unwrap()
                            .clone();
                        println!("next_search_state: {:?}", next_search_state.read().unwrap().node.id);
                        locked_response.replace(next_search_state.clone());
                        println!("transformed response: {:?}", transformed_response);
                    }
                    None => {
                        println!("search state.");
                        break;
                    }
                }
            }
            None => {
                println!("No parent_id. Breaking.");
                break;
            }
        }
    }
    transformed_response.reverse();
    println!("transformed response: {:?}", transformed_response);
    let r = response.read().unwrap().clone();
    Some((transformed_response, r.unwrap().read().unwrap().cost))
}

#[derive(Debug, Clone)]
struct SearchSate {
    cost: i64,
    node: Node,
    parent_id: Option<i64>,
}

impl PartialEq for SearchSate {
    fn eq(&self, other: &Self) -> bool {
        self.node.id.eq(&other.node.id)
    }
}

impl Eq for SearchSate {}

impl PartialOrd for SearchSate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SearchSate {
    fn cmp(&self, other: &Self) -> Ordering {
        match other.cost.cmp(&self.cost) {
            Ordering::Equal => self.cost.cmp(&other.cost),
            s => s,
        }
    }
}
