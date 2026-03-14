use std::time::Instant;

use axum::Router;
use axum::routing::{get, post};
use tokio::sync::mpsc;
use tower_http::cors::CorsLayer;

use crate::config::Config;
use crate::health;
use crate::stt;
use crate::stt::routes::SttJob;
use crate::tts;
use crate::tts::routes::TtsJob;

#[derive(Clone)]
pub struct AppState {
    pub stt_tx: mpsc::Sender<SttJob>,
    pub tts_tx: mpsc::Sender<TtsJob>,
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

    // Load both models in parallel since they are independent
    let stt_config = config.stt.clone();
    let tts_config = config.tts.clone();

    let (stt_result, tts_result) = tokio::task::spawn_blocking(move || {
        std::thread::scope(|s| {
            let stt_handle = s.spawn(|| {
                tracing::info!("initializing STT engine...");
                stt::spawn_worker(&stt_config)
            });
            let tts_handle = s.spawn(|| {
                tracing::info!("initializing TTS engine...");
                tts::spawn_worker(&tts_config)
            });
            (stt_handle.join(), tts_handle.join())
        })
    })
    .await?;

    let stt_tx = stt_result.map_err(|_| anyhow::anyhow!("STT init panicked"))??;
    let tts_tx = tts_result.map_err(|_| anyhow::anyhow!("TTS init panicked"))??;

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
