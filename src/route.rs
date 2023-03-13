use std::{error::Error, sync::mpsc};

use crate::{data::{node::Node, way::Way}, AppState};
use actix_web::{
    post,
    web::{self, Data},
    HttpResponse, Responder,
};
use pathfinding::prelude::astar;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LatLon {
    lat: f64,
    lng: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RouteRequest {
    start: LatLon,
    end: LatLon,
}

#[post("/route")]
async fn route(
    data: Data<AppState>,
    coords: web::Json<RouteRequest>,
) -> Result<impl Responder, Box<dyn Error>> {
    println!("Route request: {:?}", coords);
    let mut trx = data.db_pool.acquire().await?;
    let end = Node::closest(&mut *trx, coords.end.lat, coords.end.lng).await?;
    let ways = Way::get_with_node(&mut *trx, end.id, data.way_cache.clone()).await?;
    println!("end Ways: {:?}", ways);
    let start = Node::closest(&mut *trx, coords.start.lat, coords.start.lng).await?;

    println!("Start: {:?}", start);
    println!("End: {:?}", end);

    let (path, _score) = astar(
        &start,
        |node| -> Vec<(Node, i64)> {
            let node = node.clone();
            let data = data.clone();
            let (tx, rx) = mpsc::channel();
            let rt = data.rt.clone();
            let node_cache = data.node_cache.clone();
            let way_cache = data.way_cache.clone();
            rt.spawn(async move {
                let mut trx = data.db_pool.acquire().await.unwrap();
                let nodes = node.successors(&mut *trx, node_cache, way_cache).await.unwrap();
                tx.send(nodes).unwrap();
            });
            let r = rx.recv().unwrap();
            r
        },
        |node| node.distance(&end).into(),
        |node| node.lat == end.lat && node.lon == end.lon,
    )
    .unwrap();

    println!("Path: {:?}", path);

    let coords: Vec<LatLon> = path
        .iter()
        .map(|node| LatLon {
            lat: node.lat(),
            lng: node.lon(),
        })
        .collect();

    Ok(HttpResponse::Ok().json(coords))
}
