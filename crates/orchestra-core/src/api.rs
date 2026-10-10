use crate::domain::NewTask;
use crate::engine::{Action, Client};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::IntoResponse,
    routing::{get, post, put},
};
use serde::Deserialize;
use serde_json::{Value, json};

type Reply = Result<Json<Value>, (StatusCode, Json<Value>)>;
fn failure(message: String) -> (StatusCode, Json<Value>) {
    let code = if message == "task not found" {
        StatusCode::NOT_FOUND
    } else if message.contains("engine") {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::BAD_REQUEST
    };
    (code, Json(json!({"error":message})))
}

// Browser mutations must originate from the core or the documented dev frontend.
// No CORS headers are sent. This is a local single-user service, not authentication.
fn check_origin(headers: &HeaderMap) -> Result<(), (StatusCode, Json<Value>)> {
    if let Some(origin) = headers.get("origin") {
        let allowed = [
            "http://127.0.0.1:5173",
            "http://localhost:5173",
            "http://127.0.0.1:8080",
            "http://localhost:8080",
        ];
        if !origin.to_str().is_ok_and(|o| allowed.contains(&o)) {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({"error":"origin not allowed"})),
            ));
        }
    }
    Ok(())
}

pub fn router(client: Client) -> Router {
    Router::new()
        .route(
            "/health",
            get(|| async { Json(json!({"status":"ok", "schema_version":1})) }),
        )
        .route("/api/state", get(state))
        .route("/api/tasks", post(create))
        .route("/api/tasks/{id}/cancel", post(cancel))
        .route("/api/strategy", put(strategy))
        .route("/api/events", get(events))
        .fallback(|| async {
            (StatusCode::NOT_FOUND, Json(json!({"error":"not found"}))).into_response()
        })
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(middleware::from_fn(local_host))
        .with_state(client)
}
async fn local_host(request: Request, next: Next) -> axum::response::Response {
    let valid = request
        .headers()
        .get("host")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|host| matches!(host.split(':').next(), Some("localhost" | "127.0.0.1")));
    if !valid {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"loopback Host required"})),
        )
            .into_response();
    }
    next.run(request).await
}
async fn state(State(client): State<Client>) -> Reply {
    client
        .call(Action::Snapshot)
        .await
        .map(Json)
        .map_err(failure)
}
async fn create(
    State(client): State<Client>,
    headers: HeaderMap,
    Json(spec): Json<NewTask>,
) -> Reply {
    check_origin(&headers)?;
    client
        .call(Action::Create(spec))
        .await
        .map(Json)
        .map_err(failure)
}
async fn cancel(State(client): State<Client>, Path(id): Path<String>, headers: HeaderMap) -> Reply {
    check_origin(&headers)?;
    client
        .call(Action::Cancel(id))
        .await
        .map(Json)
        .map_err(failure)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Strategy {
    strategy: String,
}
async fn strategy(
    State(client): State<Client>,
    headers: HeaderMap,
    Json(input): Json<Strategy>,
) -> Reply {
    check_origin(&headers)?;
    client
        .call(Action::Strategy(input.strategy))
        .await
        .map(Json)
        .map_err(failure)
}
#[derive(Deserialize)]
struct Cursor {
    #[serde(default)]
    after: i64,
}
async fn events(State(client): State<Client>, Query(cursor): Query<Cursor>) -> Reply {
    client
        .call(Action::Events(cursor.after.max(0)))
        .await
        .map(Json)
        .map_err(failure)
}
