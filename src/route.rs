use crate::AppState;
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
async fn route(data: Data<AppState>, coords: web::Json<RouteRequest>) -> impl Responder {
    println!("Route request: {:?}", coords);
    let map = data.map.clone();
    let end = map.find_closest_node(coords.end.lat, coords.end.lng);
    let start = map.find_closest_node(coords.start.lat, coords.start.lng);
    println!("End node: {:?}, {:?}", end, map.node_ways.get(&end.id.0));

    let (path, _score) = astar(
        &start,
        |&node| map.successors(node),
        |node| map.distance(node, end),
        |node| node.decimicro_lat == end.decimicro_lat && node.decimicro_lon == end.decimicro_lon,
    )
    .unwrap();
    let coords: Vec<LatLon> = path
        .iter()
        .map(|node| LatLon {
            lat: node.lat(),
            lng: node.lon(),
        })
        .collect();

    HttpResponse::Ok().json(coords)
}
