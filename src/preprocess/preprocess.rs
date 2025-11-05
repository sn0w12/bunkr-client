use crate::config::config::Config;
use anyhow::{Result, anyhow};
use mime_guess::from_path;
use std::path::Path;
use std::process::Command;

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

    // Get duration
    let output = Command::new("ffprobe")
        .args(&[
            "-v", "error",
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

    let output_pattern = format!("{}_part_%03d.{}", stem, extension);

    let status = Command::new("ffmpeg")
        .args(&[
            "-i", path,
            "-f", "segment",
            "-segment_time", &segment_time.to_string(),
            "-c", "copy",
            "-reset_timestamps", "1",
            &output_pattern
        ])
        .status()?;
    if !status.success() {
        return Err(anyhow!("Failed to split video"));
    }

    let mut result = vec![];
    for i in 0..parts {
        let part_path = format!("{}_part_{:03}.{}", stem, i, extension);
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