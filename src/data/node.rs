use crate::AppState;
use actix_web::web::Data;
use postgres::Client;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, error::Error};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AdjacentNode {
    pub node_id: i64,
    pub tags: HashMap<String, String>,
}

impl AdjacentNode {
    fn has_tag_value(&self, key: &str, value: &str) -> bool {
        if let Some(v) = self.tags.get(key) {
            return v == value;
        }
        false
    }

    fn has_tag(&self, key: &str) -> bool {
        self.tags.contains_key(key)
    }
}

impl std::hash::Hash for AdjacentNode {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.node_id.hash(state);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash, Eq)]
pub struct Node {
    pub id: i64,
    /// The latitude in decimicro degrees (10⁻⁷ degrees).
    pub lat: i32,
    /// The longitude in decimicro degrees (10⁻⁷ degrees).
    pub lon: i32,
    pub adjacent_nodes: Vec<AdjacentNode>,
}

impl Node {
    pub fn get(
        pg_client: &mut Client,
        state: Data<AppState>,
        id: i64,
    ) -> Result<Self, Box<dyn Error>> {
        let node_cache = &mut state.node_cache.lock().unwrap();
        if let Some(node) = node_cache.get(&id) {
            return Ok(node.clone());
        }
        let sql = format!(
            r#"select * 
            from planet_osm_nodes n
            join planet_osm_ways  w 
                on w.nodes @> array[n.id] and tags is not null
            where 
            n.id = {}
            "#,
            id
        );
        let rows = pg_client.query(sql.as_str(), &[])?;
        let mut adjacent_nodes = vec![];
        let mut lat: i32 = 0;
        let mut lon: i32 = 0;
        for row in rows.iter() {
            lat = row.get("lat");
            lon = row.get("lon");
            // We get all the tags
            let mut tags: HashMap<String, String> = HashMap::new();
            let tag_strings: Vec<String> = row.get("tags");
            let mut ts_iter = tag_strings.iter();
            while let Some(tag) = ts_iter.next() {
                match ts_iter.next() {
                    Some(v) => tags.insert(tag.clone(), v.clone()),
                    None => tags.insert(tag.clone(), "".to_string()),
                };
            }
            println!("nodes: {:?}", row.get::<_, Vec<i64>>("nodes"));
            println!("Tags: {:?}", tags);

            // We get all the adjacent nodes
            let nodes: Vec<i64> = row.get("nodes");
            let node_index = nodes.iter().position(|&x| x == id).unwrap();

            if let Some(next_node) = nodes.get(node_index + 1) {
                adjacent_nodes.push(AdjacentNode {
                    node_id: *next_node,
                    tags: tags.clone(),
                });
            }

            // The previous one if we are not in a oneway
            if node_index > 0 {
                let prev_node = nodes.get(node_index - 1).unwrap();
                if tags.get("oneway").unwrap_or(&"".to_string()) != "yes"
                    && tags.get("oneway:bicycle").unwrap_or(&"".to_string()) != "yes"
                {
                    adjacent_nodes.push(AdjacentNode {
                        node_id: *prev_node,
                        tags: tags.clone(),
                    });
                }
            }
        }

        let node = Node {
            id,
            lat,
            lon,
            adjacent_nodes,
        };

        node_cache.insert(id, node.clone());
        Ok(node)
    }

    pub fn distance(&self, other_node: &Node) -> i32 {
        (self.lat.abs_diff(other_node.lat) + self.lon.abs_diff(other_node.lon)) as i32
    }

    pub fn closest(
        pg_client: &mut Client,
        state: Data<AppState>,
        lat: f64,
        lon: f64,
    ) -> Result<Self, Box<dyn Error>> {
        let sql = format!(
            r#"SELECT pow.nodes
            FROM planet_osm_line pol
            join planet_osm_ways pow 
              on pol.osm_id = pow.id
            where 
                pol.highway is not null and
                pol.highway != 'motorway' and
                pol.highway != 'steps' and
                pol.highway != 'track' and
                pol.aeroway is NULL and
                (pol.access != 'no' or pol.access is NULL) and
                (pol.access != 'private' or pol.access is NULL) and
                (pol.bicycle != 'no' OR pol.bicycle IS NULL)
            ORDER BY way <-> ST_Transform(ST_SetSRID(ST_MakePoint({}, {}), 4326), 3857)
            LIMIT 1"#,
            lon, lat
        );
        let node_ids: Vec<i64> = pg_client.query_one(sql.as_str(), &[])?.get("nodes");
        let mut nodes = node_ids
            .iter()
            .map(|id| {
                println!("id: {:?}", id);
                Node::get(pg_client, state.clone(), *id).unwrap()
            })
            .collect::<Vec<Node>>();
        nodes.sort_by(|a, b| {
            let a_dist =
                ((a.lat() - lat) * (a.lat() - lat) + (a.lon() - lon) * (a.lon() - lon)).sqrt();
            let b_dist =
                ((b.lat() - lat) * (b.lat() - lat) + (b.lon() - lon) * (b.lon() - lon)).sqrt();
            a_dist.partial_cmp(&b_dist).unwrap()
        });
        Ok(nodes[0].clone())
    }

    pub fn successors(
        &self,
        pg_client: &mut Client,
        state: Data<AppState>,
    ) -> Result<Vec<(Node, i64)>, Box<dyn Error>> {
        let mut nodes = Vec::new();
        for a_node in self.adjacent_nodes.clone() {
            if a_node.has_tag_value("highway", "motorway")
                || a_node.has_tag_value("bicycle", "no")
                || a_node.has_tag_value("highway", "steps")
                || a_node.has_tag_value("access", "private")
                || (!a_node.has_tag("highway") && !a_node.has_tag("bicycle"))
            {
                continue;
            }

            let winter = true;
            if winter && a_node.has_tag_value("winter_service", "no") {
                continue;
            }
            let new_node = Node::get(pg_client, state.clone(), a_node.node_id)?;
            // the score starts as the distance between the two nodes
            let mut move_cost = self.distance(&new_node) as f32;

            // We prefer cycleways
            if a_node.has_tag_value("highway", "cycleway") {
                move_cost /= 5.0;
            } else if a_node.has_tag_value("bicyle", "designated")
                || a_node.has_tag_value("bicyle", "yes")
                || a_node.has_tag_value("cycleway", "shared_lane")
                || a_node.has_tag_value("cycleway:left", "shared_lane")
                || a_node.has_tag_value("cycleway:right", "shared_lane")
                || a_node.has_tag_value("cycleway:both", "shared_lane")
                || a_node.has_tag_value("cycleway", "opposite_lane")
                || a_node.has_tag_value("cycleway:left", "opposite_lane")
                || a_node.has_tag_value("cycleway:right", "opposite_lane")
                || a_node.has_tag_value("cycleway:both", "opposite_lane")
                || a_node.has_tag_value("cycleway", "lane")
                || a_node.has_tag_value("cycleway:left", "lane")
                || a_node.has_tag_value("cycleway:right", "lane")
                || a_node.has_tag_value("cycleway:both", "lane")
                || a_node.has_tag_value("cycleway", "track")
                || a_node.has_tag_value("cycleway:left", "track")
                || a_node.has_tag_value("cycleway:right", "track")
                || a_node.has_tag_value("cycleway:both", "track")
            {
                move_cost /= 2.0;
            } else if a_node.has_tag_value("highway", "primary") {
                move_cost *= 6.0;
            } else if a_node.has_tag_value("access", "customers") {
                move_cost *= 5.0;
            } else if a_node.has_tag_value("highway", "footway") {
                move_cost *= 2.0;
            } else if a_node.has_tag_value("highway", "tertiary") {
                move_cost *= 2.0;
            } else if a_node.has_tag_value("highway", "path") {
                move_cost *= 4.0;
            } else if a_node.has_tag_value("highway", "secondary") {
                move_cost *= 3.0;
            } else if a_node.has_tag_value("highway", "service") {
                move_cost *= 3.0;
            }

            if a_node.has_tag_value("bicyle", "dismount") {
                move_cost *= 5.0;
            }
            if a_node.has_tag_value("route", "ferry") {
                move_cost *= 100.0;
            }
            nodes.push((new_node, move_cost as i64));
        }
        Ok(nodes)
    }

    pub fn lat(&self) -> f64 {
        self.lat as f64 / 10_000_000.0
    }

    pub fn lon(&self) -> f64 {
        self.lon as f64 / 10_000_000.0
    }
}

#[test]
fn test() {
    let mut pg_client = Client::connect("host=db user=osm password=osm", postgres::NoTls).unwrap();
    let node = Node::get(
        &mut pg_client,
        Data::new(AppState {
            node_cache: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
        }),
        615101618,
    )
    .unwrap();
    node.adjacent_nodes.iter().for_each(|n| {
        println!("adjacent node: {:?}", n);
    });

    assert!(false);
}
