use std::{env, error::Error, thread};

use crate::{astar::astar, data::node::Node, AppState};
use actix_web::{
    post,
    web::{self, Data},
    HttpResponse, Responder,
};
use postgres::{Client, NoTls};
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
    state: Data<AppState>,
    coords: web::Json<RouteRequest>,
) -> Result<impl Responder, Box<dyn Error>> {
    println!("Route request: {:?}", coords);

    let coords = coords.into_inner();
    let path = thread::spawn(move || {
        let mut pg_client = Client::connect(
            format!(
                "host={} user={} password={}",
                env::var("DB_HOST").unwrap(),
                env::var("DB_USER").unwrap(),
                env::var("DB_PASSWORD").unwrap()
            )
            .as_str(),
            NoTls,
        )
        .unwrap();
        let end = Node::closest(
            &mut pg_client,
            state.clone(),
            coords.end.lat,
            coords.end.lng,
        )
        .unwrap();
        let start = Node::closest(
            &mut pg_client,
            state.clone(),
            coords.start.lat,
            coords.start.lng,
        )
        .unwrap();

        println!("Start: {:?}", start);
        println!("End: {:?}", end);
        let (path, _score) = astar(start, end, state).unwrap_or((vec![], 0));
        path
    }).join().unwrap();

    path.iter().for_each(|node| println!("Node: {:?}", node.id));

    let mut response: Vec<LatLon> = path
        .iter()
        .map(|node| LatLon {
            lat: node.lat(),
            lng: node.lon(),
        })
        .collect();
    response.insert(0, coords.start.clone());
    response.push(coords.end.clone());

    Ok(HttpResponse::Ok().json(response))
}
