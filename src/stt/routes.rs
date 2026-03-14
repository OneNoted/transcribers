use axum::extract::State;
use axum_extra::extract::Multipart;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use serde::Serialize;
use tokio::sync::oneshot;

use super::audio::{TARGET_SAMPLE_RATE, decode_audio_bytes, preprocess_audio};
use crate::server::AppState;

/// Maximum upload size: 50 MB
const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;

#[derive(Debug)]
pub struct SttJob {
    pub audio: Vec<f32>,
    pub sample_rate: u32,
    pub reply: oneshot::Sender<anyhow::Result<SttResult>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SttResult {
    pub text: String,
    pub language: Option<String>,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Segment {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

pub async fn transcribe(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut audio_data: Option<Vec<u8>> = None;
    let mut response_format = ResponseFormat::Json;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                let bytes = field.bytes().await.map_err(|e| {
                    (StatusCode::BAD_REQUEST, format!("failed to read file: {e}"))
                })?;
                if bytes.len() > MAX_UPLOAD_BYTES {
                    return Err((
                        StatusCode::PAYLOAD_TOO_LARGE,
                        format!("file exceeds {MAX_UPLOAD_BYTES} byte limit"),
                    ));
                }
                audio_data = Some(bytes.to_vec());
            }
            "response_format" => {
                if let Ok(text) = field.text().await {
                    response_format = match text.as_str() {
                        "verbose_json" => ResponseFormat::VerboseJson,
                        "text" => ResponseFormat::Text,
                        _ => ResponseFormat::Json,
                    };
                }
            }
            _ => {} // Ignore model, language, prompt, and other fields
        }
    }

    let audio_data = audio_data.ok_or((StatusCode::BAD_REQUEST, "missing 'file' field".to_string()))?;

    // Offload CPU-intensive decode + preprocessing to a blocking thread
    let samples = tokio::task::spawn_blocking(move || {
        let mut samples = decode_audio_bytes(audio_data)?;
        preprocess_audio(&mut samples, TARGET_SAMPLE_RATE);
        Ok::<_, anyhow::Error>(samples)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("decode task panicked: {e}")))?
    .map_err(|e| (StatusCode::BAD_REQUEST, format!("failed to decode audio: {e}")))?;

    let (tx, rx) = oneshot::channel();
    let job = SttJob {
        audio: samples,
        sample_rate: TARGET_SAMPLE_RATE,
        reply: tx,
    };

    state.stt_tx.send(job).await.map_err(|_| {
        (StatusCode::SERVICE_UNAVAILABLE, "STT engine unavailable".to_string())
    })?;

    let result = rx.await.map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "STT worker dropped".to_string())
    })?.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("transcription failed: {e}"))
    })?;

    match response_format {
        ResponseFormat::VerboseJson => {
            Ok(Json(serde_json::json!({
                "text": result.text,
                "language": result.language,
                "segments": result.segments,
            })).into_response())
        }
        ResponseFormat::Text => Ok(result.text.into_response()),
        ResponseFormat::Json => {
            Ok(Json(serde_json::json!({ "text": result.text })).into_response())
        }
    }
}

enum ResponseFormat {
    Json,
    VerboseJson,
    Text,
}
