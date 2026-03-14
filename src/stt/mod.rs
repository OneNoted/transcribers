pub mod audio;
pub mod routes;
pub mod whisper;

use std::sync::mpsc;

use routes::{SttJob, SttResult};
use whisper::WhisperEngine;

use crate::config::SttConfig;

pub fn spawn_worker(config: &SttConfig) -> anyhow::Result<mpsc::SyncSender<SttJob>> {
    let engine = WhisperEngine::new(config)?;
    let (tx, rx) = mpsc::sync_channel::<SttJob>(4);

    std::thread::Builder::new()
        .name("stt-worker".to_string())
        .spawn(move || {
            tracing::info!("STT worker started");
            while let Ok(job) = rx.recv() {
                let result = engine
                    .transcribe(&job.audio, job.sample_rate)
                    .map(|t| SttResult {
                        text: t.text,
                        language: t.language,
                        segments: t.segments,
                    });
                let _ = job.reply.send(result);
            }
            tracing::info!("STT worker stopped");
        })?;

    Ok(tx)
}
