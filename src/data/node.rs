use crate::{
    astar::astar,
    get_pg_client,
    route::{Model, RouteRequest},
};
use serde::{Deserialize, Serialize};
use sqlx::{pool::PoolConnection, Postgres, Row};
use std::{collections::HashMap, error::Error, ops::DerefMut, sync::Arc};
use tokio::sync::{Mutex, RwLock};

fn get_positions<T: PartialEq>(iter: impl Iterator<Item = T>, elem: T) -> Vec<usize> {
    iter.enumerate()
        .filter(|(_, e)| *e == elem)
        .map(|(i, _)| i)
        .collect()
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AdjacentNode {
    pub node_id: i64,
    pub tags: HashMap<String, String>,
    pub distance: i32,
    pub intermediate_nodes: Option<Vec<i64>>,
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

pub fn distance(lat1: i32, lon1: i32, lat2: i32, lon2: i32) -> i32 {
    // We use the haversine formula
    // https://en.wikipedia.org/wiki/Haversine_formula
    // https://www.movable-type.co.uk/scripts/latlong.html
    let lat1 = lat1 as f64 / 10_000_000.0;
    let lon1 = lon1 as f64 / 10_000_000.0;
    let lat2 = lat2 as f64 / 10_000_000.0;
    let lon2 = lon2 as f64 / 10_000_000.0;
    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let a = (d_lat / 2.0).sin() * (d_lat / 2.0).sin()
        + (d_lon / 2.0).sin()
            * (d_lon / 2.0).sin()
            * lat1.to_radians().cos()
            * lat2.to_radians().cos();
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    (6_371_000.0 * c) as i32
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

lazy_static! {
    static ref NODE_CACHE: Arc<RwLock<HashMap<i64, Node>>> = Arc::new(RwLock::new(HashMap::new()));
}

impl Node {
    pub async fn get(
        pg_client: Arc<Mutex<PoolConnection<Postgres>>>,
        id: i64,
    ) -> Result<Self, Box<dyn Error>> {
        // We check if the node is in the cache
        if let Some(node) = NODE_CACHE.read().await.get(&id) {
            return Ok(node.clone());
        }

        // We get the node from the database
        let rows = sqlx::query(
            r#"
            select n.lat, n.lon, w.tags as tags , w.nodes
            from planet_osm_nodes n
            left join planet_osm_ways  w 
                on w.nodes @> array[n.id]
            where
            n.id = $1
        "#,
        )
        .bind(id)
        .fetch_all(pg_client.lock().await.deref_mut())
        .await?;
        let mut adjacent_nodes = vec![];
        let mut lat: i32 = 0;
        let mut lon: i32 = 0;
        for row in rows.iter() {
            lat = row.get("lat");
            lon = row.get("lon");
            // We get all the tags
            let mut tags: HashMap<String, String> = HashMap::new();
            let tag_strings: Vec<String> = row.try_get("tags").unwrap_or(vec![]);
            let mut ts_iter = tag_strings.iter();
            while let Some(tag) = ts_iter.next() {
                match ts_iter.next() {
                    Some(v) => tags.insert(tag.clone(), v.clone()),
                    None => tags.insert(tag.clone(), "".to_string()),
                };
            }
            // We get all the adjacent nodes
            let nodes: Vec<i64> = row.get("nodes");
            let node_indexes = get_positions(nodes.iter(), &id);
            for node_index in node_indexes {
                if let Some(next_node) = nodes.get(node_index + 1) {
                    let next_node_row = sqlx::query(
                        r#"
                        select * 
                        from planet_osm_nodes n
                        where 
                        n.id = $1
                        "#,
                    )
                    .bind(next_node)
                    .fetch_one(pg_client.lock().await.deref_mut())
                    .await?;
                    let distance =
                        distance(lat, lon, next_node_row.get("lat"), next_node_row.get("lon"));
                    adjacent_nodes.push(AdjacentNode {
                        node_id: *next_node,
                        tags: tags.clone(),
                        distance,
                        intermediate_nodes: None
                    });
                }
                // The previous one if we are not in a oneway
                if node_index > 0 {
                    let prev_node = nodes.get(node_index - 1).unwrap();
                    if !(tags.get("oneway").unwrap_or(&"".to_string()) == "yes") {
                        if !(tags.get("oneway:bycicle").unwrap_or(&"".to_string()) == "no") {
                            let previous_node_row = sqlx::query(
                                r#"
                                select * 
                                from planet_osm_nodes n
                                where 
                                n.id = $1
                                "#,
                            )
                            .bind(prev_node)
                            .fetch_one(pg_client.lock().await.deref_mut())
                            .await?;
                            let distance = distance(
                                lat,
                                lon,
                                previous_node_row.get("lat"),
                                previous_node_row.get("lon"),
                            );
                            adjacent_nodes.push(AdjacentNode {
                                node_id: *prev_node,
                                tags: tags.clone(),
                                distance,
                                intermediate_nodes: None
                            });
                        }
                    }
                }
            }
        }
        // let ways = Way::get(pg_client.clone(), id).await?;
        // for way in ways {
        //     let last_node_row = sqlx::query(
        //         r#"
        //         select * 
        //         from planet_osm_nodes n
        //         where 
        //         n.id = $1
        //         "#,
        //     )
        //     .bind(way.nodes.last().unwrap())
        //     .fetch_one(pg_client.lock().await.deref_mut())
        //     .await?;
        //     let distance = distance(lat, lon, last_node_row.get("lat"), last_node_row.get("lon"));
        //     let intermediate_nodes = Some(way.nodes);
        //     adjacent_nodes.push(AdjacentNode {
        //         node_id: last_node_row.get("id"),
        //         tags: way.tags,
        //         distance,
        //         intermediate_nodes
        //     });
        // }
        let node = Node {
            id,
            lat,
            lon,
            adjacent_nodes,
        };
        NODE_CACHE.write().await.insert(id, node.clone());
        Ok(node)
    }

    pub fn distance(&self, other_node: &Node) -> i32 {
        self::distance(self.lat, self.lon, other_node.lat, other_node.lon)
    }

    pub async fn closest(
        pg_client: Arc<Mutex<PoolConnection<Postgres>>>,
        lat: f64,
        lon: f64,
    ) -> Result<Self, Box<dyn Error>> {
        let node_ids: Vec<i64> = sqlx::query(
            r#"SELECT pow.nodes
                    FROM planet_osm_line pol
                    join planet_osm_ways pow 
                    on pol.osm_id = pow.id
                    where 
                        pol.building is NULL and
                        pol.highway is not null and
                        pol.highway != 'motorway' and
                        pol.highway != 'motorway_link' and
                        pol.highway != 'steps' and
                        pol.highway != 'track' and
                        pol.aeroway is NULL and
                        (pol.access != 'no' or pol.access is NULL) and
                        (pol.access != 'private' or pol.access is NULL) and
                        (pol.bicycle != 'no' OR pol.bicycle IS NULL)
                    ORDER BY way <-> ST_Transform(ST_SetSRID(ST_MakePoint($1, $2), 4326), 3857)
                    LIMIT 1"#,
        )
        .bind(lon)
        .bind(lat)
        .fetch_one(pg_client.lock().await.as_mut())
        .await?
        .get("nodes");

        let mut nodes = vec![];
        for id in node_ids {
            let node = Node::get(pg_client.to_owned(), id).await?;
            nodes.push(node);
        }

        nodes.sort_by(|a, b| {
            let a_dist =
                ((a.lat() - lat) * (a.lat() - lat) + (a.lon() - lon) * (a.lon() - lon)).sqrt();
            let b_dist =
                ((b.lat() - lat) * (b.lat() - lat) + (b.lon() - lon) * (b.lon() - lon)).sqrt();
            a_dist.partial_cmp(&b_dist).unwrap()
        });
        Ok(nodes[0].clone())
    }

    pub async fn successors(
        &self,
        pg_client: Arc<Mutex<PoolConnection<Postgres>>>,
        model: Model,
    ) -> Result<Vec<(Node, i64)>, Box<dyn Error>> {
        let mut nodes: Vec<(Node, i64)> = Vec::new();
        for a_node in &self.adjacent_nodes {
            if a_node.has_tag_value("highway", "motorway")
                || a_node.has_tag_value("highway", "motorway_link")
                || a_node.has_tag_value("bicycle", "no")
                || a_node.has_tag_value("highway", "steps")
                || a_node.has_tag_value("highway", "construction")
                || a_node.has_tag_value("access", "private")
                || a_node.has_tag_value("source", "approximative")
                || (!a_node.has_tag("highway") && !a_node.has_tag("bicycle"))
            {
                continue;
            }

            let winter = false;
            if winter && a_node.has_tag_value("winter_service", "no") {
                continue;
            }
            let (new_node, move_cost) = match model {
                Model::Fast => {
                    self.calculate_cost_fast(pg_client.to_owned(), a_node)
                        .await?
                }
                Model::Safe => {
                    self.calculate_cost_safe(pg_client.to_owned(), a_node)
                        .await?
                }
            };
            nodes.push((new_node, move_cost as i64));
        }
        Ok(nodes)
    }

    pub async fn calculate_cost_safe(
        &self,
        pg_client: Arc<Mutex<PoolConnection<Postgres>>>,
        a_node: &AdjacentNode,
    ) -> Result<(Node, i64), Box<dyn Error>> {
        let other_node = Node::get(pg_client.to_owned(), a_node.node_id).await?;
        let mut move_cost = a_node.distance as f64;

        if a_node.has_tag_value("route", "bicycle"){
            move_cost *= 0.8;
        }

        // We prefer cycleways
        if a_node.has_tag_value("highway", "cycleway")
            || a_node.has_tag_value("bicycle", "designated")
        {
            move_cost *= 0.7;
        } else if a_node.has_tag_value("bicycle", "yes")
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
            || a_node.has_tag_value("route", "bicycle")
        {
            move_cost *= 0.8
        } else if a_node.has_tag_value("highway", "footway") {
            if !a_node.has_tag_value("bicycle", "no") {
                move_cost *= 1.1;
            } else {
                move_cost *= 10.0;
            }
        } else if a_node.has_tag_value("surface", "gravel") {
            move_cost *= 1.2;
        } else if a_node.has_tag_value("surface", "dirt") {
            move_cost *= 5.0;
        } else if a_node.has_tag_value("bicycle", "dismount") {
            move_cost *= 3.0;
        } else if a_node.has_tag_value("highway", "tertiary") {
            move_cost *= 2.0;
        } else if a_node.has_tag_value("highway", "secondary") {
            move_cost *= 3.0;
        } else if a_node.has_tag_value("highway", "service") {
            move_cost *= 1.3;
        } else if a_node.has_tag_value("highway", "path") {
            move_cost *= 1.6;
        } else if a_node.has_tag_value("access", "customers") {
            move_cost *= 1.7;
        } else if a_node.has_tag_value("highway", "primary") {
            move_cost *= 4.0;
        } else if a_node.has_tag_value("highway", "trunk") {
            move_cost *= 4.0;
        }

        if a_node.has_tag_value("route", "ferry") {
            move_cost *= 100.0;
        }

        if let Some(speed) = a_node.tags.get("maxspeed") {
            if let Ok(speed) = speed.parse::<f32>() {
                if speed > 50.0 {
                    move_cost *= 1.2;
                }
            }
        }
        Ok((other_node, move_cost as i64))
    }

    pub async fn calculate_cost_fast(
        &self,
        pg_client: Arc<Mutex<PoolConnection<Postgres>>>,
        a_node: &AdjacentNode,
    ) -> Result<(Node, i64), Box<dyn Error>> {
        let other_node = Node::get(pg_client, a_node.node_id).await?;
        let mut move_cost = self.distance(&other_node) as f32;

        if a_node.has_tag_value("route", "bicycle"){
            move_cost *= 0.8;
        }

        // We prefer cycleways
        if a_node.has_tag_value("highway", "cycleway")
            || a_node.has_tag_value("bicycle", "designated")
        {
            move_cost *= 0.8;
        } else if a_node.has_tag_value("bicycle", "yes")
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
            move_cost *= 0.9;
        } else if a_node.has_tag_value("highway", "footway") {
            move_cost *= 1.1;
        } else if a_node.has_tag_value("surface", "gravel") {
            move_cost *= 1.1;
        } else if a_node.has_tag_value("surface", "dirt") {
            move_cost *= 5.0;
        } else if a_node.has_tag_value("bicycle", "dismount") {
            move_cost *= 3.0;
        } else if a_node.has_tag_value("highway", "tertiary") {
            move_cost *= 1.1;
        } else if a_node.has_tag_value("highway", "secondary") {
            move_cost *= 1.2;
        } else if a_node.has_tag_value("highway", "service") {
            move_cost *= 1.3;
        } else if a_node.has_tag_value("highway", "path") {
            move_cost *= 1.3;
        } else if a_node.has_tag_value("access", "customers") {
            move_cost *= 1.4;
        } else if a_node.has_tag_value("highway", "primary") {
            move_cost *= 1.3;
        } else if a_node.has_tag_value("highway", "trunk") {
            move_cost *= 1.3;
        }

        if a_node.has_tag_value("route", "ferry") {
            move_cost *= 100.0;
        }

        Ok((other_node, move_cost as i64))
    }

    pub fn lat(&self) -> f64 {
        self.lat as f64 / 10_000_000.0
    }

    pub fn lon(&self) -> f64 {
        self.lon as f64 / 10_000_000.0
    }

    pub async fn route(coords: &RouteRequest) -> Result<(Vec<Node>, i64), Box<dyn Error>> {
        let now = std::time::Instant::now();
        let coords = coords.to_owned();
        let client = Arc::new(Mutex::new(get_pg_client().await?));
        let end = Node::closest(client.to_owned(), coords.end.lat, coords.end.lng).await?;
        let start = Node::closest(client.to_owned(), coords.start.lat, coords.start.lng).await?;
        let (path, cost) = astar(
            &start,
            |node: &Node| {
                let client = client.to_owned();
                Box::pin(async move { node.successors(client, Model::Safe).await.unwrap() })
            },
            |node| node.distance(&end).into(),
            |node| {
                if now.elapsed().as_secs() > 60 {
                    return true;
                }
                node.id == end.id
            },
        )
        .await
        .expect("Problem with astar result");
        Ok((path, cost))
    }
}

// #[test]
// fn test() {
//     let mut pg_client = Client::connect("host=db user=osm password=osm", postgres::NoTls).unwrap();
//     let node = Node::get(
//         &mut pg_client,
//         Data::new(AppState {
//             node_cache: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
//         }),
//         364987802,
//     )
//     .unwrap();
//     node.adjacent_nodes.iter().for_each(|n| {
//         println!("adjacent node: {:?}", n);
//     });
//     let successors = node.successors(&mut pg_client, Data::new(AppState {
//         node_cache: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
//     })).unwrap();
//     println!("successors: {:?}", successors);

//     assert!(false);
// }
