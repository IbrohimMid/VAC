use axum::{Extension, Json};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct CreateOrder {
    pub item: String,
    pub quantity: u32,
}

#[derive(Serialize)]
pub struct Order {
    pub id: u64,
    pub item: String,
}

pub async fn create_order(
    Extension(db): Extension<DbPool>,
    Json(input): Json<CreateOrder>,
) -> Json<Order> {
    let order = db.insert(&input).await.unwrap();
    Json(order)
}
