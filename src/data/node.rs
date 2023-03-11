use futures::lock::Mutex;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use std::{collections::HashMap, error::Error, sync::Arc};

use super::way::{Way, WayError};

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone, Hash, PartialEq, Eq)]
pub struct Node {
    pub id: i64,
    /// The latitude in decimicro degrees (10⁻⁷ degrees).
    pub lat: i32,
    /// The longitude in decimicro degrees (10⁻⁷ degrees).
    pub lon: i32,
}

// Custum error type for the successor function
#[derive(Debug)]
pub enum NodeError {
    SqlxError(sqlx::Error),
}

impl Error for NodeError {
    fn description(&self) -> &str {
        match self {
            NodeError::SqlxError(_e) => "SqlxError",
        }
    }
}

impl std::fmt::Display for NodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            NodeError::SqlxError(e) => write!(f, "SqlxError: {}", e),
        }
    }
}

impl From<WayError> for NodeError {
    fn from(error: WayError) -> Self {
        match error {
            WayError::SqlxError(e) => NodeError::SqlxError(e),
        }
    }
}

impl From<sqlx::Error> for NodeError {
    fn from(error: sqlx::Error) -> Self {
        NodeError::SqlxError(error)
    }
}

impl Node {
    pub async fn get(
        trx: &mut PgConnection,
        id: i64,
        node_cache: &Mutex<HashMap<i64, Node>>,
    ) -> Result<Self, NodeError> {
        let mut node_cache = node_cache.lock().await;
        if let Some(node) = node_cache.get(&id) {
            return Ok(node.clone());
        }
        let sql = format!(
            r#"select * 
                  from planet_osm_nodes 
                  where id = {}"#,
            id
        );
        let node: Node = sqlx::query_as(sql.as_str()).fetch_one(trx).await?;
        node_cache.insert(id, node.clone());
        Ok(node)
    }

    pub fn distance(&self, other_node: &Node) -> i32 {
        (self.lat.abs_diff(other_node.lat) + self.lon.abs_diff(other_node.lon)) as i32
    }

    pub async fn closest(trx: &mut PgConnection, lat: f64, lon: f64) -> Result<Self, NodeError> {
        let sql = format!(
            r#"SELECT pow.*
            FROM planet_osm_line pol
            join planet_osm_ways pow 
              on pol.osm_id = pow.id
            ORDER BY way <-> ST_Transform(ST_SetSRID(ST_MakePoint({}, {}), 4326), 3857)
            LIMIT 1"#,
            lon, lat
        );
        let way: Way = sqlx::query_as(sql.as_str()).fetch_one(&mut *trx).await?;
        let mut nodes_in = "(".to_string();
        for (i, node) in way.nodes.iter().enumerate() {
            nodes_in.push_str(node.to_string().as_str());
            if i < way.nodes.len() - 1 {
                nodes_in.push(',');
            }
        }
        nodes_in.push(')');
        let sql = format!(
            r#"SELECT *
                    FROM planet_osm_nodes 
                    WHERE id IN {} "#,
            nodes_in
        );
        let mut nodes: Vec<Node> = sqlx::query_as(sql.as_str())
            .bind(nodes_in)
            .fetch_all(trx)
            .await?;
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
        trx: &mut PgConnection,
        node_cache: Arc<Mutex<HashMap<i64, Node>>>,
        way_cache: Arc<Mutex<HashMap<i64, Vec<Way>>>>,
    ) -> Result<Vec<(Node, i64)>, NodeError> {
        println!("successors({})", self.id);
        let mut nodes = Vec::new();
        let ways = Way::get_with_node(trx, self.id, way_cache).await?;
        println!("ways({})", ways.len());
        for way in ways {
            let node_index = way
                .nodes
                .iter()
                .position(|node_id| *node_id == self.id)
                .unwrap();
            for (i, node_id) in way.nodes.iter().enumerate() {
                // we keep just the nodes that are next to the current node
                if (i as i32 - node_index as i32).abs() != 1 {
                    continue;
                }
                // do not go in one way in the opposite direction
                if !way.has_tag_value("oneway:bicycle", "no")
                    && way.has_tag_value("oneway", "yes")
                    && (i as i32 - node_index as i32) != 1
                {
                    continue;
                }

                if way.has_tag_value("highway", "motorway")
                    || way.has_tag_value("bicycle", "no")
                    || way.has_tag_value("highway", "steps")
                    || (!way.has_tag("highway") && !way.has_tag("bicycle"))
                {
                    continue;
                }
                let winter = true;
                if winter && way.has_tag_value("winter_service", "no") {
                    continue;
                }
                println!("node_id({})", node_id);
                let new_node = Node::get(trx, *node_id, &node_cache).await?;
                println!("new_node({})", new_node.id);
                // the score starts as the distance between the two nodes
                let mut move_cost = self.distance(&new_node) as f32;

                // We prefer cycleways
                if way.has_tag_value("highway", "cycleway") {
                    move_cost /= 3.0;
                } else if way.has_tag_value("bicyle", "designated")
                    || way.has_tag_value("bicyle", "yes")
                    || way.has_tag_value("cycleway", "shared_lane")
                    || way.has_tag_value("cycleway:left", "shared_lane")
                    || way.has_tag_value("cycleway:right", "shared_lane")
                    || way.has_tag_value("cycleway:both", "shared_lane")
                    || way.has_tag_value("cycleway", "opposite_lane")
                    || way.has_tag_value("cycleway:left", "opposite_lane")
                    || way.has_tag_value("cycleway:right", "opposite_lane")
                    || way.has_tag_value("cycleway:both", "opposite_lane")
                    || way.has_tag_value("cycleway", "lane")
                    || way.has_tag_value("cycleway:left", "lane")
                    || way.has_tag_value("cycleway:right", "lane")
                    || way.has_tag_value("cycleway:both", "lane")
                    || way.has_tag_value("cycleway", "track")
                    || way.has_tag_value("cycleway:left", "track")
                    || way.has_tag_value("cycleway:right", "track")
                    || way.has_tag_value("cycleway:both", "track")
                {
                    move_cost /= 2.0;
                } else if way.has_tag_value("highway", "primary") {
                    move_cost *= 6.0;
                } else if way.has_tag_value("access", "customers") {
                    move_cost *= 5.0;
                } else if way.has_tag_value("bicyle", "dismount") {
                    move_cost *= 5.0;
                } else if way.has_tag_value("highway", "footway") {
                    move_cost *= 2.0;
                } else if way.has_tag_value("highway", "tertiary") {
                    move_cost *= 2.0;
                } else if way.has_tag_value("highway", "path") {
                    move_cost *= 4.0;
                } else if way.has_tag_value("highway", "secondary") {
                    move_cost *= 3.0;
                }

                if way.has_tag_value("route", "ferry") {
                    move_cost *= 100.0;
                }
                nodes.push((new_node, move_cost as i64));
            }
        }
        println!("nodes({})", nodes.len());
        Ok(nodes)
    }

    pub fn lat(&self) -> f64 {
        self.lat as f64 / 10_000_000.0
    }

    pub fn lon(&self) -> f64 {
        self.lon as f64 / 10_000_000.0
    }
}

#[tokio::test]
async fn test() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect("postgres://osm:osm@db/osm")
        .await
        .unwrap();

    let trx = &mut pool.begin().await.unwrap();
    let node = Node::closest(trx, 45.46085305860483, -73.59282016754152)
        .await
        .unwrap_or_else(|e| {
            println!("{:?}", e);
            panic!("error")
        });
    println!("{:?}", node);
    assert!(false);
}
