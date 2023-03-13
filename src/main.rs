use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use actix_cors::Cors;
use actix_web::web::Data;
use actix_web::{App, HttpServer};
use data::node::Node;

mod data;
mod route;

pub struct AppState {
    node_cache: Arc<Mutex<HashMap<i64, Node>>>,
}

#[actix_web::main] // or #[tokio::main]
async fn main() -> std::io::Result<()> {
    let node_cache = Arc::new(Mutex::new(HashMap::new()));
    
    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header();
        App::new()
            .app_data(Data::new(AppState {
                node_cache: node_cache.clone(),
            }))
            .wrap(cors)
            .service(route::route)
    })
    .bind(("0.0.0.0", 3000))?
    .run()
    .await
}
