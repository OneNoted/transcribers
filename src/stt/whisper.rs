use serde::{Deserialize, Serialize};
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, get_lang_str,
};

use crate::config::SttConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub text: String,
    pub language: Option<String>,
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

impl Transcript {
    fn empty(language: Option<String>) -> Self {
        Self {
            text: String::new(),
            language,
            segments: Vec::new(),
        }
    }

    fn append_chunk(&mut self, mut chunk: Transcript, offset_ms: u32) {
        if self.language.is_none() {
            self.language = chunk.language.take();
        }

        let chunk_text = chunk.text.trim();
        if !chunk_text.is_empty() {
            if !self.text.is_empty() {
                self.text.push(' ');
            }
            self.text.push_str(chunk_text);
        }

        for mut segment in chunk.segments {
            segment.start += offset_ms as f64 / 1000.0;
            segment.end += offset_ms as f64 / 1000.0;
            self.segments.push(segment);
        }
    }
}

pub struct WhisperEngine {
    ctx: WhisperContext,
    language: String,
}

impl WhisperEngine {
    pub fn new(config: &SttConfig) -> anyhow::Result<Self> {
        let model_path = &config.model_path;
        anyhow::ensure!(
            model_path.exists(),
            "model file not found: {}",
            model_path.display()
        );

        tracing::info!("loading whisper model from {}", model_path.display());

        let mut ctx_params = WhisperContextParameters::default();
        ctx_params.use_gpu(config.use_gpu);
        ctx_params.flash_attn(config.use_gpu && config.flash_attn);
        tracing::info!(
            "whisper acceleration: use_gpu={}, flash_attn={}",
            config.use_gpu,
            config.use_gpu && config.flash_attn
        );

        let model_path_str = model_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("model path contains invalid UTF-8"))?;

        let ctx = WhisperContext::new_with_params(model_path_str, ctx_params)
            .map_err(|e| anyhow::anyhow!("failed to load whisper model: {e}"))?;

        tracing::info!("whisper model loaded");

        Ok(Self {
            ctx,
            language: config.language.clone(),
        })
    }

    pub fn transcribe(&self, audio: &[f32], sample_rate: u32) -> anyhow::Result<Transcript> {
        if audio.is_empty() || sample_rate == 0 {
            return Ok(Transcript::empty(language_hint(&self.language)));
        }

        let duration_secs = audio.len() as f64 / sample_rate as f64;
        let rms = (audio.iter().map(|s| s * s).sum::<f32>() / audio.len() as f32).sqrt();
        tracing::info!("audio: {:.1}s, {} samples, RMS={:.4}", duration_secs, audio.len(), rms);

        if duration_secs < 0.3 {
            return Ok(Transcript::empty(language_hint(&self.language)));
        }
        if rms < 0.01 {
            return Ok(Transcript::empty(language_hint(&self.language)));
        }

        let chunk_size = (30.0 * sample_rate as f64) as usize;
        let overlap = (1.0 * sample_rate as f64) as usize;

        if audio.len() <= chunk_size {
            self.transcribe_chunk(audio)
        } else {
            let mut transcript = Transcript::empty(language_hint(&self.language));
            let mut offset = 0;

            while offset < audio.len() {
                let end = (offset + chunk_size).min(audio.len());
                let chunk = &audio[offset..end];
                let chunk_transcript = self.transcribe_chunk(chunk)?;
                let offset_ms = samples_to_ms(offset, sample_rate);
                transcript.append_chunk(chunk_transcript, offset_ms);

                if end == audio.len() {
                    break;
                }
                offset = end - overlap;
            }

            Ok(transcript)
        }
    }

    fn transcribe_chunk(&self, audio: &[f32]) -> anyhow::Result<Transcript> {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 3 });

        if self.language == "auto" {
            params.set_language(None);
        } else {
            params.set_language(Some(&self.language));
        }
        params.set_translate(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        let n_threads = std::thread::available_parallelism()
            .map(|n| n.get() as i32)
            .unwrap_or(4);
        params.set_n_threads(n_threads);

        let mut state = self.ctx.create_state()
            .map_err(|e| anyhow::anyhow!("failed to create whisper state: {e}"))?;

        state.full(params, audio)
            .map_err(|e| anyhow::anyhow!("transcription failed: {e}"))?;

        let language = if self.language == "auto" {
            get_lang_str(state.full_lang_id_from_state()).map(ToOwned::to_owned)
        } else {
            Some(self.language.clone())
        };

        let mut text = String::new();
        let mut segments = Vec::new();
        let num_segments = state.full_n_segments();
        for i in 0..num_segments {
            let Some(segment) = state.get_segment(i) else {
                continue;
            };

            let seg_text = read_segment_text(i, &segment);
            let trimmed = seg_text.trim();
            if trimmed.is_empty() {
                continue;
            }

            text.push_str(&seg_text);
            segments.push(TranscriptSegment {
                text: trimmed.to_string(),
                start: centiseconds_to_secs(segment.start_timestamp()),
                end: centiseconds_to_secs(segment.end_timestamp()),
            });
        }

        Ok(Transcript {
            text: text.trim().to_string(),
            language,
            segments,
        })
    }
}

fn read_segment_text(i: i32, segment: &whisper_rs::WhisperSegment<'_>) -> String {
    match segment.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => match segment.to_str_lossy() {
            Ok(lossy) => {
                tracing::warn!("segment {i} contains invalid UTF-8, using lossy conversion");
                lossy.to_string()
            }
            Err(_) => String::new(),
        },
    }
}

fn language_hint(language: &str) -> Option<String> {
    (language != "auto").then(|| language.to_string())
}

fn centiseconds_to_secs(value: i64) -> f64 {
    value as f64 / 100.0
}

fn samples_to_ms(samples: usize, sample_rate: u32) -> u32 {
    if sample_rate == 0 {
        return 0;
    }
    ((samples as u64).saturating_mul(1000) / sample_rate as u64).min(u32::MAX as u64) as u32
}
