use std::{error::Error, sync::Arc, collections::HashMap};

use futures::lock::Mutex;
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection};

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct Way {
    pub id: i64,
    pub nodes: Vec<i64>,
    pub tags: Vec<String>,
}

#[derive(Debug)]
pub enum WayError {
    SqlxError(sqlx::Error),
}

impl Error for WayError {
    fn description(&self) -> &str {
        match self {
            WayError::SqlxError(_e) => "SqlxError",
        }
    }
}

impl std::fmt::Display for WayError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            WayError::SqlxError(e) => write!(f, "SqlxError: {}", e),
        }
    }
}

impl From<sqlx::Error> for WayError {
    fn from(error: sqlx::Error) -> Self {
        WayError::SqlxError(error)
    }
}

impl Way {
    pub async fn get_with_node(
        trx: &mut PgConnection,
        node_id: i64,
        way_cache: Arc<Mutex<HashMap<i64, Vec<Way>>>>,
    ) -> Result<Vec<Self>, WayError> {
        let mut way_cache = way_cache.lock().await;
        if let Some(ways) = way_cache.get(&node_id) {
            return Ok(ways.clone());
        }
        let sql = format!(
            r#"select * 
            from planet_osm_ways pow 
            where nodes @> array[cast({} as bigint)] and tags is not null"#, node_id
        );
        let ways: Vec<Way> = sqlx::query_as(sql.as_str())
        .fetch_all(trx)
        .await.unwrap();
        way_cache.insert(node_id, ways.clone());
        Ok(ways.clone())
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
    pub fn has_tag_value(&self, tag: &str, value: &str) -> bool {
        self.tags
            .iter()
            .enumerate()
            .any(|(i, t)| t == tag && self.tags[i + 1] == value)
    }
}
