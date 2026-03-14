use axum::extract::State;
use axum_extra::extract::Multipart;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use serde::Serialize;
use tokio::sync::oneshot;

use super::audio::{TARGET_SAMPLE_RATE, decode_audio_bytes, preprocess_audio};
#[allow(unused_imports)]
use super::whisper::TranscriptSegment;
use crate::server::AppState;

#[derive(Debug)]
pub struct SttJob {
    pub audio: Vec<f32>,
    pub sample_rate: u32,
    pub reply: oneshot::Sender<anyhow::Result<SttResult>>,
}

#[derive(Debug, Clone)]
pub struct SttResult {
    pub text: String,
    pub language: Option<String>,
    pub segments: Vec<super::whisper::TranscriptSegment>,
}

#[derive(Serialize)]
struct TranscriptionResponse {
    text: String,
}

#[derive(Serialize)]
struct VerboseTranscriptionResponse {
    text: String,
    language: Option<String>,
    segments: Vec<VerboseSegment>,
}

#[derive(Serialize)]
struct VerboseSegment {
    text: String,
    start: f64,
    end: f64,
}

pub async fn transcribe(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut audio_data: Option<Vec<u8>> = None;
    let mut response_format = "json".to_string();
    let mut language: Option<String> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                let bytes = field.bytes().await.map_err(|e| {
                    (StatusCode::BAD_REQUEST, format!("failed to read file: {e}"))
                })?;
                audio_data = Some(bytes.to_vec());
            }
            "response_format" => {
                if let Ok(text) = field.text().await {
                    response_format = text;
                }
            }
            "language" => {
                if let Ok(text) = field.text().await {
                    language = Some(text);
                }
            }
            _ => {} // Ignore model and other fields
        }
    }

    let audio_data = audio_data.ok_or((StatusCode::BAD_REQUEST, "missing 'file' field".to_string()))?;

    let mut samples = decode_audio_bytes(&audio_data)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("failed to decode audio: {e}")))?;

    preprocess_audio(&mut samples, TARGET_SAMPLE_RATE);

    let (tx, rx) = oneshot::channel();
    let job = SttJob {
        audio: samples,
        sample_rate: TARGET_SAMPLE_RATE,
        reply: tx,
    };

    // Override language if provided in request
    let _ = language; // TODO: per-request language override

    state.stt_tx.send(job).map_err(|_| {
        (StatusCode::SERVICE_UNAVAILABLE, "STT engine unavailable".to_string())
    })?;

    let result = rx.await.map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "STT worker dropped".to_string())
    })?.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("transcription failed: {e}"))
    })?;

    match response_format.as_str() {
        "verbose_json" => {
            Ok(Json(serde_json::to_value(VerboseTranscriptionResponse {
                text: result.text,
                language: result.language,
                segments: result.segments.into_iter().map(|s| VerboseSegment {
                    text: s.text,
                    start: s.start,
                    end: s.end,
                }).collect(),
            }).unwrap()).into_response())
        }
        "text" => Ok(result.text.into_response()),
        _ => {
            Ok(Json(TranscriptionResponse { text: result.text }).into_response())
        }
    }
}
