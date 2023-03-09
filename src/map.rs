use mongodb::Client;
use osmpbfreader::{Node, OsmObj, OsmPbfReader, Way};
use std::{collections::HashMap, fs::File, sync::Arc};

#[derive(Clone, Debug)]
pub struct Map {
    pub nodes: HashMap<i64, Node>,
    pub node_ways: HashMap<i64, Vec<Arc<Way>>>,
    pub ways: HashMap<i64, Arc<Way>>,
}

impl Map {
    pub fn load(file_name: &str, mongo_client: Client) -> Map {
        let mut nodes = HashMap::new();
        let mut node_ways = HashMap::new();
        let mut ways = HashMap::new();
        let f = File::open(file_name).unwrap();
        let mut reader = OsmPbfReader::new(f);
        reader.par_iter().for_each(|o| match o.unwrap() {
            OsmObj::Node(n) => {
                nodes.insert(n.id.0, n);
            }
            OsmObj::Way(w) => {
                let w = Arc::new(w);
                ways.insert(w.id.0, w.clone());
                w.nodes.iter().for_each(|n| {
                    if !node_ways.contains_key(&n.0) {
                        node_ways.insert(n.0, Vec::new());
                    }
                    node_ways.get_mut(&n.0).unwrap().push(w.clone());
                });
            }
            OsmObj::Relation(_relation) => {}
        });
        Map {
            nodes,
            node_ways,
            ways,
        }
    }

    pub fn successors(&self, node: &Node) -> Vec<(&Node, i32)> {
        let mut nodes = Vec::new();
        let ways = self.node_ways.get(&node.id.0).unwrap();
        for way in ways {
            let node_index = way.nodes.iter().position(|n| n.0 == node.id.0).unwrap();
            for (i, node_id) in way.nodes.iter().enumerate() {
                // we keep just the nodes that are next to the current node
                if (i as i32 - node_index as i32).abs() != 1 {
                    continue;
                }
                // do not go in one way in the opposite direction
                if !way.tags.contains("oneway:bicycle", "no")
                    && way.tags.contains("oneway", "yes")
                    && (i as i32 - node_index as i32) != 1
                {
                    continue;
                }

                if way.tags.contains("highway", "motorway")
                    || way.tags.contains("bicycle", "no")
                    || way.tags.contains("highway", "steps")
                    || (!way.tags.contains_key("highway") && !way.tags.contains_key("bicycle"))
                {
                    continue;
                }
                let new_node = self.nodes.get(&node_id.0).unwrap();
                // the score starts as the distance between the two nodes
                let mut move_cost = self.distance(node, new_node) as f32;

                // We prefer cycleways
                if way.tags.contains("highway", "cycleway") {
                    move_cost /= 3.0;
                } else if way.tags.contains("bicyle", "designated")
                    || way.tags.contains("bicyle", "yes")
                    || way.tags.contains("cycleway", "lane")
                    || way.tags.contains("cycleway", "shared_lane")
                    || way.tags.contains("cycleway:left", "lane")
                    || way.tags.contains("cycleway:right", "lane")
                {
                    move_cost /= 2.0;
                } else if way.tags.contains("highway", "primary") {
                    move_cost *= 6.0;
                } else if way.tags.contains("access", "customers") {
                    move_cost *= 5.0;
                } else if way.tags.contains("bicyle", "dismount") {
                    move_cost *= 5.0;
                } else if way.tags.contains("highway", "footway") {
                    move_cost *= 2.0;
                } else if way.tags.contains("highway", "tertiary") {
                    move_cost *= 2.0;
                } else if way.tags.contains("highway", "path") {
                    move_cost *= 4.0;
                } else if way.tags.contains("highway", "secondary") {
                    move_cost *= 3.0;
                }

                if way.tags.contains("route", "ferry") {
                    move_cost *= 5.0;
                }
                nodes.push((new_node, move_cost as i32));
            }
        }
        // we return a vector of tuples with (node, move_cost)
        nodes
    }

    pub fn distance(&self, node: &Node, end: &Node) -> i32 {
        (node.decimicro_lat.abs_diff(end.decimicro_lat)
            + node.decimicro_lon.abs_diff(end.decimicro_lon)) as i32
    }

    pub fn find_closest_node(&self, lat: f64, lon: f64) -> &Node {
        let mut closest_node = None;
        let mut closest_distance = std::f64::MAX;
        for node in self.nodes.values() {
            match self.node_ways.get(&node.id.0) {
                None => continue,
                Some(way) => {
                    if !way.first().unwrap().tags.contains_key("highway") {
                        continue;
                    }
                    if way.first().unwrap().tags.contains("bicycle", "no") {
                        continue;
                    }
                }
            }
            let distance = (node.lat() - lat).abs() + (node.lon() - lon).abs();
            if distance < closest_distance {
                closest_distance = distance;
                closest_node = Some(node);
            }
        }
        closest_node.unwrap()
    }
}
