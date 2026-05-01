use std::collections::HashMap;
use std::path::Path;
use serde_json::json;

use crate::tools::{Tool, ToolContext};

/// Transcribe audio or video files using ffmpeg + whisper.cpp (or whisper via Ollama if available).
pub struct TranscribeTool;

impl TranscribeTool {
    pub fn new() -> Self {
        TranscribeTool
    }

    /// Check if ffmpeg is available on PATH.
    fn has_ffmpeg() -> bool {
        which::which("ffmpeg").is_ok()
    }

    /// Check if a whisper executable is available on PATH.
    fn has_whisper() -> Option<std::path::PathBuf> {
        for name in ["whisper", "whisper.cpp", "whisper-cli", "main"] {
            if let Ok(path) = which::which(name) {
                return Some(path);
            }
        }
        None
    }

    /// Extract audio from a video file to a temporary WAV using ffmpeg.
    fn extract_audio(video_path: &Path, output_wav: &Path) -> Result<(), String> {
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-i", video_path.to_str().unwrap_or(""),
                "-vn",
                "-acodec", "pcm_s16le",
                "-ar", "16000",
                "-ac", "1",
                "-y",
                output_wav.to_str().unwrap_or(""),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|e| format!("ffmpeg failed to start: {}", e))?;

        if !status.success() {
            return Err("ffmpeg exited with non-zero status".to_string());
        }
        Ok(())
    }

    /// Run whisper.cpp on a WAV file and return the transcript.
    fn run_whisper(whisper_exe: &Path, wav_path: &Path) -> Result<String, String> {
        let model = std::env::var("AVALON_WHISPER_MODEL")
            .unwrap_or_else(|_| "models/ggml-base.bin".to_string());

        let output = std::process::Command::new(whisper_exe)
            .args([
                "-m", &model,
                "-f", wav_path.to_str().unwrap_or(""),
                "--output-txt",
                "--no-timestamps",
            ])
            .output()
            .map_err(|e| format!("whisper failed to start: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("whisper error: {}", stderr));
        }

        // whisper.cpp writes output to .txt next to the input file
        let txt_path = wav_path.with_extension("txt");
        let transcript = std::fs::read_to_string(&txt_path)
            .map_err(|e| format!("Failed to read whisper output: {}", e))?;
        let _ = std::fs::remove_file(&txt_path);
        Ok(transcript.trim().to_string())
    }
}

#[async_trait::async_trait]
impl Tool for TranscribeTool {
    fn name(&self) -> &str {
        "transcribe"
    }

    fn description(&self) -> &str {
        "Transcribe an audio or video file. Input: {\"path\": \"path/to/file.mp4\", \"title\": \"optional title\"}"
    }

    fn is_core(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> Result<serde_json::Value, String> {
        let path_str = input.get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing 'path' parameter".to_string())?;
        let title = input.get("title").and_then(|v| v.as_str());
        let path = Path::new(path_str);

        // Validate path is in allowed_paths
        if !ctx.fs.config().is_allowed(path_str) {
            return Err(format!(
                "Path '{}' is not in allowed_paths. Add it to filesystem configuration to enable access.",
                path_str
            ));
        }

        if !path.exists() {
            return Err(format!("File not found: {}", path_str));
        }

        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let is_video = matches!(ext.as_str(), "mp4" | "webm" | "avi" | "mov" | "mkv");
        let is_audio = matches!(ext.as_str(), "mp3" | "wav" | "ogg" | "flac" | "m4a" | "aac");

        if !is_video && !is_audio {
            return Err(format!("Unsupported file format: {}", ext));
        }

        // Always ingest the raw file first
        let raw_id = ctx.vault.lock().unwrap().ingest_file(path, title, None, "Public", None)
            .map_err(|e| e.to_string())?;

        let mut result = HashMap::<String, serde_json::Value>::new();
        result.insert("file_id".to_string(), json!(raw_id));
        result.insert("file_path".to_string(), json!(path_str));

        // Check for transcription tools
        let ffmpeg_ok = Self::has_ffmpeg();
        let whisper_exe = Self::has_whisper();

        if !ffmpeg_ok {
            result.insert("status".to_string(), json!("ingested_no_transcription"));
            result.insert("reason".to_string(), json!("ffmpeg not found on PATH"));
            return Ok(json!(result));
        }

        // Prepare audio path
        let temp_dir = std::env::temp_dir();
        let audio_path = temp_dir.join(format!("avalon_transcribe_{}.wav", raw_id));

        let extract_result = if is_video {
            Self::extract_audio(path, &audio_path)
        } else {
            // For audio files, convert to standard WAV if needed
            if ext == "wav" {
                std::fs::copy(path, &audio_path).map_err(|e| e.to_string())?;
                Ok(())
            } else {
                Self::extract_audio(path, &audio_path)
            }
        };

        if let Err(e) = extract_result {
            result.insert("status".to_string(), json!("ingested_audio_extract_failed"));
            result.insert("reason".to_string(), json!(e));
            let _ = std::fs::remove_file(&audio_path);
            return Ok(json!(result));
        }

        if let Some(whisper) = whisper_exe {
            match Self::run_whisper(&whisper, &audio_path) {
                Ok(transcript) => {
                    if transcript.is_empty() {
                        result.insert("status".to_string(), json!("ingested_empty_transcript"));
                    } else {
                        // Store transcription as a related vault item
                        let now = chrono::Utc::now().to_rfc3339();
                        let trans_id = ctx.vault.lock().unwrap().ingest_text(
                            &format!("{}#transcript", path_str),
                            title.map(|t| format!("{} (transcript)", t)).as_deref(),
                            &transcript,
                            "text",
                            "Public",
                            None,
                        ).map_err(|e| e.to_string())?;

                        // Link transcription to original file
                        let _ = ctx.vault.lock().unwrap().link_items(
                            trans_id, raw_id, "summarizes", 1.0, Some("whisper transcription"),
                        );

                        result.insert("status".to_string(), json!("transcribed"));
                        result.insert("transcript_id".to_string(), json!(trans_id));
                        result.insert("transcript_preview".to_string(), json!(
                            if transcript.len() > 200 { format!("{}...", &transcript[..200]) } else { transcript.clone() }
                        ));
                    }
                }
                Err(e) => {
                    result.insert("status".to_string(), json!("ingested_whisper_failed"));
                    result.insert("reason".to_string(), json!(e));
                }
            }
        } else {
            result.insert("status".to_string(), json!("ingested_no_whisper"));
            result.insert("reason".to_string(), json!("whisper executable not found on PATH. Install whisper.cpp and set AVALON_WHISPER_MODEL if needed."));
        }

        let _ = std::fs::remove_file(&audio_path);
        Ok(json!(result))
    }
}
