use std::{error::Error, thread, sync::mpsc};

use crate::{
    data::{node::Node},
    AppState,
};
use actix_web::{
    post,
    web::{self, Data},
    HttpResponse, Responder,
};
use pathfinding::prelude::astar;
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
    let now = std::time::Instant::now();
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let mut pg_client = Client::connect("host=db user=osm password=osm", NoTls).unwrap();
        let end = Node::closest(&mut pg_client, state.clone(), coords.end.lat, coords.end.lng).unwrap();
        let start = Node::closest(&mut pg_client, state.clone(), coords.start.lat, coords.start.lng).unwrap();
    
        println!("Start: {:?}", start);
        println!("End: {:?}", end);
    
            let (path, _score) = astar(
            &start,
            |node| -> Vec<(Node, i64)> {
                node.successors(&mut pg_client, state.clone()).unwrap()
            },
            |node| node.distance(&end).into(),
            |node| {
                if now.elapsed().as_secs() > 45 {
                    return true;
                }
                node.lat == end.lat && node.lon == end.lon
            },
        )
        .unwrap();
        tx.send(path).unwrap();
    });

    let path = rx.recv().unwrap();
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
