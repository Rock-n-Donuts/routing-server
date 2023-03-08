use actix_cors::Cors;
use actix_web::web::Data;
use actix_web::{App, HttpServer};
use mongodb::Client;
use std::sync::{Arc};
use map::Map;
mod route;
mod map;


struct AppState {
    mongo_client: Client,
    map: Arc<Map>,
}



#[actix_web::main] // or #[tokio::main]
async fn main() -> std::io::Result<()> {
    let mongo_client = Client::with_uri_str("mongodb://osm:osm@mongo:27017")
        .await
        .unwrap();
    let map = Arc::new(Map::load("montreal.pbf", mongo_client.clone()));
    println!("Map loaded");
    
    HttpServer::new(move|| {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header();
        App::new()
            .app_data(Data::new(AppState {
                mongo_client: mongo_client.clone(),
                map: map.clone(),
            }))
            .wrap(cors)
            .service(route::route)
    })
    .bind(("0.0.0.0", 3000))?
    .run()
    .await
}
