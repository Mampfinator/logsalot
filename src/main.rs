#![feature(let_chains)]

use sqlx::sqlite::SqlitePoolOptions;

mod client;
mod commands;
mod logging;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    let db_uri = std::env::var("DATABASE_URL").unwrap_or_else(|_| panic!("No DB_URI passed."));

    let pool = SqlitePoolOptions::new().connect(&db_uri).await.unwrap();

    sqlx::migrate!().run(&pool).await.unwrap();

    let mut client = client::get_client(pool).await;

    client.start().await.unwrap()
}
