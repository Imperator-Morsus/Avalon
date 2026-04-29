use serde_json;
use crate::tools::{Tool, ToolContext};
use std::path::Path;
use std::process::Command;
use base64::Engine;

pub struct VideoAnalyzeTool;

#[derive(Debug, Clone, serde::Serialize)]
struct VideoMetadata {
    path: String,
    duration_seconds: f64,
    width: u32,
    height: u32,
    codec: String,
    bitrate: String,
    fps: f64,
    has_audio: bool,
    has_subtitles: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
struct VideoFrame {
    timestamp_seconds: f64,
    base64: String,
    mime_type: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct VideoAnalysis {
    metadata: VideoMetadata,
    frames: Vec<VideoFrame>,
    transcript: Option<String>,
    warnings: Vec<String>,
}

#[async_trait::async_trait]
impl Tool for VideoAnalyzeTool {
    fn name(&self) -> &str {
        "analyze_video"
    }

    fn description(&self) -> &str {
        "Analyzes a local video file. Extracts metadata (duration, resolution, codec), keyframes at regular intervals, and any embedded subtitle track. Returns frames as base64 images for visual analysis. Requires ffmpeg to be installed."
    }

    fn is_core(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext<'_>,
    ) -> Result<serde_json::Value, String> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'path' argument")?;

        if !Path::new(path).exists() {
            return Err(format!("Video file not found: {}", path));
        }

        // Check ffmpeg is available
        if Command::new("ffmpeg").arg("-version").output().is_err() {
            return Err(
                "ffmpeg is not installed or not on PATH. \
                 Install it from https://ffmpeg.org/download.html and try again."
                    .to_string(),
            );
        }

        let interval = input
            .get("interval_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(30) as u32;

        let max_frames = input
            .get("max_frames")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as usize;

        let mut warnings = Vec::new();

        // ── Metadata via ffprobe ──
        let meta = extract_metadata(path)?;

        // ── Temp directory ──
        let temp_dir = std::env::temp_dir()
            .join(format!(
                "avalon_video_{}_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                std::process::id()
            ));
        let _ = std::fs::create_dir_all(&temp_dir);
        let temp_dir_cleanup = TempDirCleanup(&temp_dir);

        // ── Extract frames ──
        let frame_count = std::cmp::min(
            max_frames,
            (meta.duration_seconds / interval.max(1) as f64).ceil() as usize,
        );

        if frame_count == 0 {
            warnings.push("Video too short to extract any frames.".to_string());
        }

        let mut frames = Vec::new();
        for i in 0..frame_count {
            let timestamp = (i as u32 * interval) as f64;
            if timestamp >= meta.duration_seconds {
                break;
            }
            match extract_frame(path, timestamp, &temp_dir) {
                Ok(frame_path) => {
                    let ext = Path::new(&frame_path)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("jpg");
                    let mime = match ext.to_lowercase().as_str() {
                        "png" => "image/png",
                        _ => "image/jpeg",
                    };
                    match std::fs::read(&frame_path) {
                        Ok(bytes) => {
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                            frames.push(VideoFrame {
                                timestamp_seconds: timestamp,
                                base64: b64,
                                mime_type: mime.to_string(),
                            });
                        }
                        Err(e) => {
                            warnings.push(format!("Failed to read frame at {}s: {}", timestamp, e));
                        }
                    }
                }
                Err(e) => {
                    warnings.push(format!("Failed to extract frame at {}s: {}", timestamp, e));
                }
            }
        }

        // ── Extract embedded subtitles ──
        let transcript = if meta.has_subtitles {
            match extract_subtitles(path, &temp_dir) {
                Ok(text) if !text.trim().is_empty() => Some(text),
                _ => None,
            }
        } else {
            None
        };

        drop(temp_dir_cleanup);
        let _ = std::fs::remove_dir_all(&temp_dir);

        let analysis = VideoAnalysis {
            metadata: meta,
            frames,
            transcript,
            warnings,
        };

        serde_json::to_value(analysis).map_err(|e| e.to_string())
    }
}

struct TempDirCleanup<'a>(&'a std::path::PathBuf);

fn extract_metadata(path: &str) -> Result<VideoMetadata, String> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=width,height,codec_name,r_frame_rate,bit_rate",
            "-show_entries", "format=duration",
            "-of", "json",
            path,
        ])
        .output()
        .map_err(|e| format!("ffprobe failed: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffprobe error: {}", err));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse ffprobe output: {}", e))?;

    let stream = json.get("streams").and_then(|s| s.get(0)).cloned().unwrap_or_default();
    let format = json.get("format").cloned().unwrap_or_default();

    let width = stream.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let height = stream.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let codec = stream.get("codec_name").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
    let bitrate = stream.get("bit_rate").and_then(|v| v.as_str()).unwrap_or("0").to_string();
    let duration = format.get("duration").and_then(|v| v.as_str()).unwrap_or("0").parse::<f64>().unwrap_or(0.0);

    let fps_str = stream.get("r_frame_rate").and_then(|v| v.as_str()).unwrap_or("0/1");
    let fps = parse_fps(fps_str);

    // Check audio
    let audio_output = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-select_streams", "a:0",
            "-show_entries", "stream=codec_type",
            "-of", "csv=p=0",
            path,
        ])
        .output();
    let has_audio = audio_output.map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains("audio")).unwrap_or(false);

    // Check subtitles
    let sub_output = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-select_streams", "s:0",
            "-show_entries", "stream=index",
            "-of", "csv=p=0",
            path,
        ])
        .output();
    let has_subtitles = sub_output.map(|o| o.status.success() && !String::from_utf8_lossy(&o.stdout).trim().is_empty()).unwrap_or(false);

    Ok(VideoMetadata {
        path: path.to_string(),
        duration_seconds: duration,
        width,
        height,
        codec,
        bitrate,
        fps,
        has_audio,
        has_subtitles,
    })
}

fn parse_fps(s: &str) -> f64 {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() == 2 {
        let num = parts[0].parse::<f64>().unwrap_or(0.0);
        let den = parts[1].parse::<f64>().unwrap_or(1.0);
        if den != 0.0 {
            return num / den;
        }
    }
    s.parse::<f64>().unwrap_or(0.0)
}

fn extract_frame(video_path: &str, timestamp: f64, out_dir: &std::path::Path) -> Result<String, String> {
    let out_file = out_dir.join(format!("frame_{:.2}.jpg", timestamp));
    let out_str = out_file.to_string_lossy().to_string();

    let status = Command::new("ffmpeg")
        .args([
            "-ss", &format!("{:.3}", timestamp),
            "-i", video_path,
            "-frames:v", "1",
            "-q:v", "2",
            "-y",
            &out_str,
        ])
        .output()
        .map_err(|e| format!("ffmpeg failed: {}", e))?;

    if !status.status.success() {
        let err = String::from_utf8_lossy(&status.stderr);
        return Err(format!("ffmpeg error: {}", err));
    }

    Ok(out_str)
}

fn extract_subtitles(video_path: &str, out_dir: &std::path::Path) -> Result<String, String> {
    let out_file = out_dir.join("subtitles.srt");
    let out_str = out_file.to_string_lossy().to_string();

    let status = Command::new("ffmpeg")
        .args([
            "-i", video_path,
            "-map", "0:s:0",
            "-f", "srt",
            "-y",
            &out_str,
        ])
        .output()
        .map_err(|e| format!("ffmpeg failed: {}", e))?;

    if !status.status.success() {
        // Try ass/ssa format
        let status2 = Command::new("ffmpeg")
            .args([
                "-i", video_path,
                "-map", "0:s:0",
                "-f", "ass",
                "-y",
                &out_str,
            ])
            .output()
            .map_err(|e| format!("ffmpeg failed: {}", e))?;
        if !status2.status.success() {
            return Err("No supported subtitle track found.".to_string());
        }
    }

    std::fs::read_to_string(&out_file)
        .map_err(|e| format!("Failed to read subtitles: {}", e))
}
