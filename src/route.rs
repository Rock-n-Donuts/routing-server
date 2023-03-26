use std::{
    error::Error,
    sync::{Arc, Mutex},
    thread,
};

use crate::{data::node::Node, get_pg_client, AppState};
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
    state: Data<AppState>,
    coords: web::Json<RouteRequest>,
) -> Result<impl Responder, Box<dyn Error>> {
    println!("Route request: {:?}", coords);
    let now = std::time::Instant::now();

    let coords = coords.into_inner();
    let state_clone = state.clone();
    let (path, _cost) = thread::spawn(move || {
        let pg_client = Arc::new(Mutex::new(get_pg_client().unwrap()));
        let state = state_clone;
        let end = Node::closest(
            pg_client.clone(),
            state.clone(),
            coords.end.lat,
            coords.end.lng,
        )
        .unwrap();
        let start = Node::closest(
            pg_client.clone(),
            state.clone(),
            coords.start.lat,
            coords.start.lng,
        )
        .unwrap();

        println!("Start: {:?}", start);
        println!("End: {:?}", end);
        let (path, cost) = astar(
            &start,
            |node| -> Vec<(Node, i64)> {
                node.successors(pg_client.clone(), state.clone()).unwrap()
            },
            |node| node.distance(&end).into(),
            |node| {
                if now.elapsed().as_secs() > 45 {
                    return true;
                }
                node.id == end.id
            },
        )
        .unwrap();
        println!("Path: {:?}", path);
        // Save the path to the database as a shortcut
        // Shortcut::save(pg_client.clone(), path.clone(), cost).unwrap();
        (path, cost)
    })
    .join()
    .unwrap_or_else(|e| {
        println!("Could get the path data from the thread {:?}", e);
        panic!();
    });

    let mut response: Vec<LatLon> = thread::spawn(move || {
        let mut response = vec![];
        path.iter().for_each(|node| {
            response.push(LatLon {
                lat: node.lat(),
                lng: node.lon(),
            })
        });
        response
    })
    .join()
    .unwrap();

    response.insert(0, coords.start.clone());
    response.push(coords.end.clone());

    Ok(HttpResponse::Ok().json(response))
}
