use actix_cors::Cors;
use actix_web::web::Data;
use actix_web::{App, HttpServer};
use data::node::Node;
use sqlx::pool::PoolConnection;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::{env, thread};

#[macro_use]
extern crate lazy_static;

mod data;
mod route;
mod astar;

pub struct AppState {
    node_cache: Arc<RwLock<HashMap<i64, Node>>>,
}

#[actix_web::main] // or #[tokio::main]
async fn main() -> std::io::Result<()> {
    let node_cache = Arc::new(RwLock::new(HashMap::new()));

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

lazy_static! {
    static ref DB_POOL: Pool<Postgres> = {
        let url: String = format!(
            "postgres://{}:{}@{}/{}",
            env::var("DB_USER").unwrap(),
            env::var("DB_PASSWORD").unwrap(),
            env::var("DB_HOST").unwrap(),
            env::var("DB_DATABASE").unwrap(),
        );

        thread::spawn(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                PgPoolOptions::new()
                    .max_connections(15)
                    .connect(&url)
                    .await
                    .unwrap()
            })
        })
        .join()
        .expect("Problem in the pool creation thread")
    };
}

async fn get_pg_client() -> Result<PoolConnection<Postgres>, sqlx::Error> {
    DB_POOL.acquire().await
}

