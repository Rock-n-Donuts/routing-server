use std::{
    error::Error,
    thread,
};

use crate::{data::node::Node};
use actix_web::{
    post,
    web::{self},
    HttpResponse, Responder,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LatLon {
    pub lat: f64,
    pub lng: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Model {
    Fast,
    Safe,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RouteRequest {
    pub start: LatLon,
    pub end: LatLon,
    pub model: Model,
}

#[post("/route")]
async fn route(
    coords: web::Json<RouteRequest>,
) -> Result<impl Responder, Box<dyn Error>> {
    let coords = coords.into_inner();
    let (path, _cost) = Node::route(&coords).await?;
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
