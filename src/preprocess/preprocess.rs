use crate::config::config::Config;
use anyhow::{Result, anyhow};
use mime_guess::from_path;
use std::path::Path;
use std::process::Command;
use uuid::Uuid;

pub struct PreprocessResult {
    pub files_to_upload: Vec<String>,
    pub preprocess_id: String,
}

pub fn preprocess_file(path: &str, max_file_size: u64, config: &Config) -> Result<PreprocessResult> {
    let p = Path::new(path);
    let mime = from_path(p).first_or_octet_stream();

    // Video preprocessing
    if mime.type_() == mime_guess::mime::VIDEO && config.preprocess_videos.unwrap_or(true) {
        let metadata = p.metadata()?;
        let size = metadata.len();
        if size > max_file_size {
            let parts = split_video(path, max_file_size)?;
            return Ok(PreprocessResult {
                files_to_upload: parts,
                preprocess_id: "split_video".to_string(),
            });
        }
    }

    // Default: no preprocessing
    Ok(PreprocessResult {
        files_to_upload: vec![path.to_string()],
        preprocess_id: "original".to_string(),
    })
}

pub fn cleanup_preprocess(preprocess_id: &str, _original_path: &str, files_to_upload: &[String]) {
    match preprocess_id {
        "original" => {
            // Nothing to clean up
        }
        "split_video" => {
            for file in files_to_upload {
                let _ = std::fs::remove_file(file);
            }
            // Try to remove the temp directory if it's empty
            if let Some(first_file) = files_to_upload.first() {
                if let Some(parent) = Path::new(first_file).parent() {
                    let _ = std::fs::remove_dir(parent);
                }
            }
        }
        _ => {
            // Unknown preprocess, do nothing
        }
    }
}

fn split_video(path: &str, max_file_size: u64) -> Result<Vec<String>> {
    let p = Path::new(path);
    let stem = p.file_stem().unwrap().to_string_lossy();
    let extension = p.extension().unwrap_or_default().to_string_lossy();

    let parent_dir = p.parent().unwrap_or(Path::new("."));
    let temp_dir = parent_dir.join(format!("bunkr_split_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir)?;

    let hwaccel = detect_hwaccel();
    let output = Command::new("ffprobe")
        .args(&[
            "-v", "quiet",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            path
        ])
        .output()?;
    if !output.status.success() {
        return Err(anyhow!("Failed to get video duration: {}", String::from_utf8_lossy(&output.stderr)));
    }
    let duration_str = String::from_utf8(output.stdout)?;
    let duration: f64 = duration_str.trim().parse()?;

    let metadata = p.metadata()?;
    let size = metadata.len();
    let parts = (size as f64 / max_file_size as f64).ceil() as u32;
    let segment_time = duration / parts as f64;
    let output_pattern = temp_dir.join(format!("{}_%03d.{}", stem, extension)).to_string_lossy().to_string();

    // Build ffmpeg args
    let mut args = vec![];
    if let Some(accel) = hwaccel {
        args.push("-hwaccel".to_string());
        args.push(accel);
    }
    args.push("-loglevel".to_string());
    args.push("quiet".to_string());
    args.push("-i".to_string());
    args.push(path.to_string());
    args.push("-f".to_string());
    args.push("segment".to_string());
    args.push("-segment_time".to_string());
    args.push(segment_time.to_string());
    args.push("-c".to_string());
    args.push("copy".to_string());
    args.push("-reset_timestamps".to_string());
    args.push("1".to_string());
    args.push(output_pattern);

    let status = Command::new("ffmpeg")
        .args(&args)
        .status()?;
    if !status.success() {
        return Err(anyhow!("Failed to split video"));
    }

    let mut result = vec![];
    for i in 0..parts {
        let part_path = temp_dir.join(format!("{}_{:03}.{}", stem, i, extension)).to_string_lossy().to_string();
        // Check if file exists and size <= max_file_size
        if let Ok(meta) = std::fs::metadata(&part_path) {
            if meta.len() <= max_file_size {
                result.push(part_path);
            } else {
                // If still too big, perhaps further split, but for now, include anyway
                result.push(part_path);
            }
        }
    }

    Ok(result)
}

fn detect_hwaccel() -> Option<String> {
    let output = Command::new("ffmpeg").arg("-hwaccels").output();
    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let lines: Vec<&str> = stdout.lines().collect();
            if let Some(pos) = lines.iter().position(|l| l.contains("Hardware acceleration methods:")) {
                for line in lines.iter().skip(pos + 1) {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() && trimmed != "none" {
                        return Some(trimmed.to_string());
                    }
                }
            }
        }
        _ => {}
    }
    None
}