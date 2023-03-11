use std::collections::HashMap;
use std::sync::{Arc};

use actix_cors::Cors;
use actix_web::web::Data;
use actix_web::{App, HttpServer};
use data::node::Node;
use data::way::Way;
use futures::lock::Mutex;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use tokio::runtime::Runtime;
mod data;
mod route;

struct AppState {
    db_pool: Pool<Postgres>,
    rt: Arc<Runtime>,
    node_cache: Arc<Mutex<HashMap<i64, Node>>>,
    way_cache: Arc<Mutex<HashMap<i64, Vec<Way>>>>
}

#[actix_web::main] // or #[tokio::main]
async fn main() -> std::io::Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(50)
        .connect("postgres://osm:osm@db/osm")
        .await
        .unwrap();

    let rt = Arc::new(Runtime::new().unwrap());
    let node_cache = Arc::new(Mutex::new(HashMap::new()));
    let way_cache = Arc::new(Mutex::new(HashMap::new()));
    
    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header();
        App::new()
            .app_data(Data::new(AppState {
                db_pool: pool.clone(),
                rt: rt.clone(),
                node_cache: node_cache.clone(),
                way_cache: way_cache.clone()
            }))
            .wrap(cors)
            .service(route::route)
    })
    .bind(("0.0.0.0", 3000))?
    .run()
    .await
}
