use futures::TryStreamExt;
use sqlx::{pool::PoolConnection, Postgres, Row};
use std::{collections::HashMap, error::Error, sync::Arc};
use tokio::sync::Mutex;

use crate::get_pg_client;

#[derive(sqlx::FromRow, Debug)]
pub struct Way {
    pub id: i64,
    pub nodes: Vec<i64>,
    pub tags: HashMap<String, String>,
    pub distance: Option<i64>,
}

impl Way {
    pub async fn get(
        client: Arc<Mutex<PoolConnection<Postgres>>>,
        node_id: i64,
    ) -> Result<Vec<Way>, Box<dyn Error>> {
        let rows = sqlx::query(
            r#"
                    select pow.*, wl.length  
                    from ways_length wl 
                    join planet_osm_ways pow
                    on pow.id = wl.ways_id 
                    where wl.first_node = $1
                    and pow.tags is not null
                "#,
        )
        .bind(node_id)
        .fetch_all(client.lock().await.as_mut())
        .await?;
        let mut ways = vec![];
        for row in rows {
            let mut tags: HashMap<String, String> = HashMap::new();
            let tag_strings: Vec<String> = row.get("tags");
            let mut ts_iter = tag_strings.iter();
            while let Some(tag) = ts_iter.next() {
                match ts_iter.next() {
                    Some(v) => tags.insert(tag.clone(), v.clone()),
                    None => tags.insert(tag.clone(), "".to_string()),
                };
            }
            ways.push(Way {
                id: row.get("id"),
                nodes: row.get("nodes"),
                tags,
                distance: row.get("length"),
            });
        }
        Ok(ways)
    }

    pub async fn calculate_all_lengths(
        client: Arc<Mutex<PoolConnection<Postgres>>>,
    ) -> Result<(), Box<dyn Error>> {
        let mut unlocked_client = client.lock().await;
        let mut stream = sqlx::query(
            r#"
                select pow.id, nodes, pow.tags as wtags, por.tags as rtags
                from planet_osm_ways pow
                left join ways_length wl
                on pow.id = wl.ways_id
                left join planet_osm_rels por
                on por.parts @> array[pow.id]
            "#,
        )
        .fetch(unlocked_client.as_mut());
        while let Some(row) = stream.try_next().await? {
            let client = Arc::new(Mutex::new(get_pg_client().await?));
            let id: i64 = row.get("id");
            let node_ids: Vec<i64> = row.get("nodes");
            let mut length = 0;
            for i in 0..node_ids.len() - 1 {
                let node1_row = sqlx::query(
                    r#"
                        select *
                        from planet_osm_nodes pon
                        where id = $1;
                    "#,
                )
                .bind(node_ids[i])
                .fetch_one(client.lock().await.as_mut())
                .await?;
                let node2_row = sqlx::query(
                    r#"
                        select *
                        from planet_osm_nodes pon
                        where id = $1;
                    "#,
                )
                .bind(node_ids[i + 1])
                .fetch_one(client.lock().await.as_mut())
                .await?;
                length += crate::data::node::distance(
                    node1_row.get("lat"),
                    node1_row.get("lon"),
                    node2_row.get("lat"),
                    node2_row.get("lon"),
                );
            }
            let mut tags: Vec<String> = row.try_get("wtags").unwrap_or(vec![]);
            tags.append(&mut row.try_get("rtags").unwrap_or(vec![]));
            sqlx::query(
                r#"
                    insert into ways_length (ways_id, length, first_node, last_node, tags_way_and_rel)
                    values ($1, $2, $3, $4, $5)
                    on conflict (ways_id) 
                    do update
                    set length = $2, first_node = $3, last_node = $4, tags_way_and_rel = $5
                    where ways_length.ways_id = $1;
                "#,
            )
            .bind(id)
            .bind(length)
            .bind(node_ids.first().unwrap())
            .bind(node_ids.last().unwrap())
            .bind(tags)
            .execute(client.lock().await.as_mut())
            .await?;
        }
        Ok(())
    }
}

#[tokio::test]
async fn get_way() {
    use crate::get_pg_client;
    let time = std::time::Instant::now();
    let client = get_pg_client().await.unwrap();
    let way = Way::get(Arc::new(Mutex::new(client)), 503820608)
        .await
        .unwrap();
    println!("it took: {:?}", time.elapsed());
    println!("way: {:?}", way);
    assert_eq!(2, 1);
}

#[tokio::test]
async fn calculate_all_lengths() {
    use crate::get_pg_client;
    let time = std::time::Instant::now();
    let client = get_pg_client().await.unwrap();
    Way::calculate_all_lengths(Arc::new(Mutex::new(client)))
        .await
        .unwrap();
    println!("it took: {:?}", time.elapsed());
    assert_eq!(2, 1);
}
