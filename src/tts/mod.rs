pub mod routes;

use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::Context;
use candle_core::Device;
use qwen3_tts::{Qwen3TTS, SynthesisOptions, auto_device, hub::ModelPaths};
use speakers_core::lang;
use speakers_core::model::ModelVariant;
use speakers_core::profile::{self, ProfileMode};
use speakers_core::protocol::VoiceSelection;

use crate::config::TtsConfig;
use routes::TtsJob;

pub fn spawn_worker(config: &TtsConfig) -> anyhow::Result<mpsc::SyncSender<TtsJob>> {
    let model_variant = match config.model.as_str() {
        "base" => ModelVariant::Base,
        _ => ModelVariant::CustomVoice,
    };

    let timeout = Duration::from_millis(config.synthesis_timeout_ms.max(1));

    tracing::info!("loading TTS model: {}", model_variant.model_id());
    let device = auto_device().unwrap_or_else(|e| {
        tracing::warn!("GPU init failed ({e}), falling back to CPU");
        Device::Cpu
    });

    let model_paths = ModelPaths::download(Some(model_variant.model_id()))
        .context("failed to download TTS model")?;
    let model = Qwen3TTS::from_paths(&model_paths, device.clone())
        .context("failed to load TTS model")?;

    tracing::info!("TTS model loaded: {:?}, device={device:?}", model_variant);

    let (tx, rx) = mpsc::sync_channel::<TtsJob>(4);

    std::thread::Builder::new()
        .name("tts-worker".to_string())
        .spawn(move || {
            tracing::info!("TTS worker started");
            while let Ok(job) = rx.recv() {
                let result = synthesize(
                    &model,
                    &device,
                    model_variant,
                    timeout,
                    &job.text,
                    &job.voice,
                    &job.language,
                );
                let _ = job.reply.send(result);
            }
            tracing::info!("TTS worker stopped");
        })?;

    Ok(tx)
}

fn synthesize(
    model: &Qwen3TTS,
    device: &Device,
    model_variant: ModelVariant,
    timeout: Duration,
    text: &str,
    voice: &VoiceSelection,
    language: &str,
) -> anyhow::Result<Vec<u8>> {
    let text = text.trim();
    anyhow::ensure!(!text.is_empty(), "text is empty");

    let language = lang::parse_language(language)?;

    let started = Instant::now();

    let audio = match voice {
        VoiceSelection::Preset { name } => {
            anyhow::ensure!(
                model_variant == ModelVariant::CustomVoice,
                "preset voices require custom-voice model, but running {model_variant}"
            );
            let speaker = lang::parse_speaker(name)?;
            model.synthesize_with_voice(text, speaker, language, None)?
        }
        VoiceSelection::Profile { name } => {
            anyhow::ensure!(
                model_variant == ModelVariant::Base,
                "profile voices require base model, but running {model_variant}"
            );
            let prompt = profile::load_profile(name, device)?;
            let meta = profile::read_profile_meta(name)?;
            let is_icl = meta.mode == ProfileMode::Icl;
            let options = profile_synthesis_options(is_icl);
            model.synthesize_voice_clone(text, &prompt, language, Some(options))?
        }
    };

    let elapsed = started.elapsed();
    anyhow::ensure!(
        elapsed <= timeout,
        "synthesis exceeded timeout ({}ms > {}ms)",
        elapsed.as_millis(),
        timeout.as_millis()
    );

    // Encode to WAV in memory
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 24000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)?;
        for &sample in &audio.samples {
            let clamped = sample.clamp(-1.0, 1.0);
            let i16_val = (clamped * i16::MAX as f32) as i16;
            writer.write_sample(i16_val)?;
        }
        writer.finalize()?;
    }

    let duration_secs = audio.samples.len() as f64 / 24000.0;
    tracing::info!(
        "synthesized {:.1}s audio in {:.1}s (RTF {:.2})",
        duration_secs,
        elapsed.as_secs_f64(),
        elapsed.as_secs_f64() / duration_secs
    );

    Ok(cursor.into_inner())
}

fn profile_synthesis_options(is_icl: bool) -> SynthesisOptions {
    let mut options = SynthesisOptions {
        temperature: 0.35,
        top_k: 20,
        top_p: 0.75,
        repetition_penalty: 1.2,
        seed: Some(42),
        ..SynthesisOptions::default()
    };
    if is_icl {
        options.max_length = 240;
        options.repetition_penalty = options.repetition_penalty.max(1.35);
    }
    options
}
