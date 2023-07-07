use actix_cors::Cors;
use actix_web::{App, HttpServer};
use sqlx::pool::PoolConnection;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::{env, thread};

#[macro_use]
extern crate lazy_static;

mod astar;
mod data;
mod route;

#[actix_web::main] // or #[tokio::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header();
        App::new()
            .wrap(cors)
            .service(route::route)
    })
    .bind(("0.0.0.0", 3000))?
    .run()
    .await
}

lazy_static! {
    static ref DB_POOL: Pool<Postgres> = {
        let url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

        thread::spawn(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                let pool = PgPoolOptions::new()
                    .max_connections(15)
                    .connect(&url)
                    .await
                    .unwrap();
                sqlx::migrate!().run(&pool).await.unwrap();
                pool
            })
        })
        .join()
        .expect("Problem in the pool creation thread")
    };
}

async fn get_pg_client() -> Result<PoolConnection<Postgres>, sqlx::Error> {
    DB_POOL.acquire().await
}
