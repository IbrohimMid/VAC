use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct UpdateUser { pub name: String }

#[derive(Serialize)]
pub struct User { pub id: u64, pub name: String }

// vil-lsp: handler uses Json<T> — expected ShmSlice
// vil-lsp: returns Json(user) — expected VilResponse
pub async fn update_user(Json(input): Json<UpdateUser>) -> Json<User> {
    Json(User { id: 1, name: input.name })
}
