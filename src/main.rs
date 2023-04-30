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

#[tokio::test]
async fn test_sqlx() {
    use tokio::sync::Mutex;
    let client = Arc::new(Mutex::new(get_pg_client().await.unwrap()));
    let end = Node::closest(
        client,
        Data::new(AppState {
            node_cache: Arc::new(RwLock::new(HashMap::new())),
        }),
        45.45954453431156,
        -73.57355117797853,
    )
    .await
    .unwrap();
    assert_eq!(end.id, 1);
}

// #[tokio::test]
// async fn test_node_get() {
//     use crate::route::Model;
//     use crate::astar::astar;

//     let client = get_pg_client().await;
//     let state = Data::new(AppState {
//         node_cache: Arc::new(RwLock::new(HashMap::new())),
//     });

//     let start = Node::closest(
//         client.to_owned(),
//         Data::new(AppState {
//             node_cache: Arc::new(RwLock::new(HashMap::new())),
//         }),
//         45.4615895,
//         -73.5835502,
//     )
//     .await
//     .unwrap();

//     let end = Node::closest(
//         client.to_owned(),
//         Data::new(AppState {
//             node_cache: Arc::new(RwLock::new(HashMap::new())),
//         }),
//         45.46059639799132,
//         -73.58367919921876,
//     )
//     .await
//     .unwrap();

//     let (_path, _cost) = astar(
//         &start,
//         |node: &Node|  async move {
//             node.successors(client.to_owned(), state, Model::Safe).await.unwrap()
//         }.boxed(),
//         |node| node.distance(&end).into(),
//         |node| {
//             node.id == end.id
//         }
//     ).await.unwrap();

//     let node2 = start.to_owned();
//     let successors = thread::spawn(move || {
//         tokio::runtime::Runtime::new().unwrap().block_on(async {
//             node2
//                 .successors(client.to_owned(), state, Model::Safe)
//                 .await
//                 .unwrap()
//         })
//     });
//     println!("-----> {:?}", successors);
//     assert_eq!(2187138724, end.id);
// }
