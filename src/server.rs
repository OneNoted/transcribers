use std::sync::mpsc;
use std::time::Instant;

use axum::Router;
use axum::routing::{get, post};
use tower_http::cors::CorsLayer;

use crate::config::Config;
use crate::health;
use crate::stt;
use crate::stt::routes::SttJob;
use crate::tts;
use crate::tts::routes::TtsJob;

#[derive(Clone)]
pub struct AppState {
    pub stt_tx: mpsc::SyncSender<SttJob>,
    pub tts_tx: mpsc::SyncSender<TtsJob>,
    pub stt_model: String,
    pub tts_model: String,
    pub default_voice: String,
    pub started: Instant,
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    let started = Instant::now();

    let stt_model = config
        .stt
        .model_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("whisper")
        .to_string();

    let tts_model = format!("qwen3-tts-{}", config.tts.model);

    tracing::info!("initializing STT engine...");
    let stt_tx = stt::spawn_worker(&config.stt)?;

    tracing::info!("initializing TTS engine...");
    let tts_tx = tts::spawn_worker(&config.tts)?;

    let state = AppState {
        stt_tx,
        tts_tx,
        stt_model,
        tts_model,
        default_voice: config.tts.default_voice.clone(),
        started,
    };

    let app = Router::new()
        .route("/health", get(health::health))
        .route("/v1/models", get(health::models))
        .route("/v1/audio/transcriptions", post(stt::routes::transcribe))
        .route("/v1/audio/speech", post(tts::routes::speech))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.server.listen).await?;
    tracing::info!("listening on {}", config.server.listen);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C handler");
    tracing::info!("shutdown signal received");
}
