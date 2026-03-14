use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::server::AppState;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub stt_model: String,
    pub tts_model: String,
    pub uptime_secs: u64,
}

pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        stt_model: state.stt_model.clone(),
        tts_model: state.tts_model.clone(),
        uptime_secs: state.started.elapsed().as_secs(),
    })
}

pub async fn models(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "object": "list",
        "data": [
            {
                "id": state.stt_model,
                "object": "model",
            },
            {
                "id": state.tts_model,
                "object": "model",
            },
        ]
    }))
}
