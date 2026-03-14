mod config;
mod health;
mod server;
mod stt;
mod tts;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "transcribers", about = "Unified STT + TTS HTTP server")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the HTTP server
    Serve {
        /// Path to config file
        #[arg(short, long, default_value = "transcribers.toml")]
        config: PathBuf,
    },
    /// Transcribe an audio file (offline test)
    Transcribe {
        /// Audio file to transcribe
        file: PathBuf,
        /// Language hint (default: auto)
        #[arg(short, long, default_value = "auto")]
        language: String,
    },
    /// Synthesize text to an audio file (offline test)
    Speak {
        /// Text to synthesize
        text: String,
        /// Voice name
        #[arg(short, long, default_value = "ryan")]
        voice: String,
        /// Output WAV path
        #[arg(short, long, default_value = "output.wav")]
        output: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Serve { config: config_path } => {
            let config = match config::Config::load(&config_path) {
                Ok(c) => c,
                Err(_) => {
                    tracing::warn!("config not found at {}, using defaults", config_path.display());
                    config::Config::default()
                }
            };
            server::run(config).await?;
        }
        Command::Transcribe { file, language } => {
            let stt_config = config::SttConfig {
                language,
                ..Default::default()
            };
            let engine = stt::whisper::WhisperEngine::new(&stt_config)?;
            let mut samples = stt::audio::decode_audio_file(&file)?;
            stt::audio::preprocess_audio(&mut samples, stt::audio::TARGET_SAMPLE_RATE);
            let transcript = engine.transcribe(&samples, stt::audio::TARGET_SAMPLE_RATE)?;
            println!("{}", transcript.text);
        }
        Command::Speak { text, voice, output } => {
            let tts_config = config::TtsConfig {
                default_voice: voice.clone(),
                ..Default::default()
            };
            let tts_tx = tts::spawn_worker(&tts_config)?;
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
            let voice_selection = tts::routes::parse_voice(&voice, &tts_config.default_voice);
            tts_tx.send(tts::routes::TtsJob {
                text,
                voice: voice_selection,
                language: "en".to_string(),
                reply: reply_tx,
            }).await?;
            let wav_bytes = reply_rx.await??;
            std::fs::write(&output, &wav_bytes)?;
            println!("wrote {}", output.display());
        }
    }

    Ok(())
}
