use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use speakers_core::protocol::VoiceSelection;
use tokio::sync::oneshot;

use crate::server::AppState;

/// Maximum TTS input text length (characters).
const MAX_INPUT_LEN: usize = 4096;

#[derive(Debug)]
pub struct TtsJob {
    pub text: String,
    pub voice: VoiceSelection,
    pub language: String,
    pub reply: oneshot::Sender<anyhow::Result<Vec<u8>>>,
}

#[derive(Deserialize)]
pub struct SpeechRequest {
    pub input: String,
    #[serde(default = "default_voice")]
    pub voice: String,
}

fn default_voice() -> String {
    "ryan".to_string()
}

pub async fn speech(
    State(state): State<AppState>,
    Json(req): Json<SpeechRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if req.input.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "input text is empty".to_string()));
    }
    if req.input.len() > MAX_INPUT_LEN {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("input exceeds {MAX_INPUT_LEN} character limit"),
        ));
    }

    let voice = parse_voice(&req.voice, &state.default_voice);
    let language = "en".to_string();

    let (tx, rx) = oneshot::channel();
    let job = TtsJob {
        text: req.input,
        voice,
        language,
        reply: tx,
    };

    state.tts_tx.send(job).await.map_err(|_| {
        (StatusCode::SERVICE_UNAVAILABLE, "TTS engine unavailable".to_string())
    })?;

    let wav_bytes = rx.await.map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "TTS worker dropped".to_string())
    })?.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("synthesis failed: {e}"))
    })?;

    Ok((
        [(header::CONTENT_TYPE, "audio/wav")],
        wav_bytes,
    ))
}

/// Parse a voice string into a `VoiceSelection`.
/// Supports "profile:NAME" for cloned voices, otherwise treats as a preset.
pub fn parse_voice(voice_str: &str, default: &str) -> VoiceSelection {
    let voice_str = if voice_str.is_empty() { default } else { voice_str };

    if let Some(profile_name) = voice_str.strip_prefix("profile:") {
        VoiceSelection::profile(profile_name)
    } else {
        VoiceSelection::preset(voice_str)
    }
}
