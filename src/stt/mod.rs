pub mod audio;
pub mod routes;
pub mod whisper;

use tokio::sync::mpsc;

use routes::{Segment, SttJob, SttResult};
use whisper::WhisperEngine;

use crate::config::SttConfig;

pub fn spawn_worker(config: &SttConfig) -> anyhow::Result<mpsc::Sender<SttJob>> {
    let engine = WhisperEngine::new(config)?;
    let (tx, mut rx) = mpsc::channel::<SttJob>(4);

    std::thread::Builder::new()
        .name("stt-worker".to_string())
        .spawn(move || {
            tracing::info!("STT worker started");
            while let Some(job) = rx.blocking_recv() {
                let result = engine
                    .transcribe(&job.audio, job.sample_rate)
                    .map(|t| SttResult {
                        text: t.text,
                        language: t.language,
                        segments: t.segments.into_iter().map(|s| Segment {
                            text: s.text,
                            start: s.start,
                            end: s.end,
                        }).collect(),
                    });
                let _ = job.reply.send(result);
            }
            tracing::info!("STT worker stopped");
        })?;

    Ok(tx)
}
