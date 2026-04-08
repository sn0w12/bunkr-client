use crate::core::types::{AlbumFile, FailedOperationInfo};
#[cfg(feature = "download")]
use crate::core::types::DownloadResponse;
use anyhow::{Result, anyhow};
#[cfg(feature = "download")]
use base64::{Engine as _, engine::general_purpose};
#[cfg(feature = "download")]
use json5;
use std::sync::{Arc, Mutex};
#[cfg(feature = "download")]
use reqwest::{Client, header};
#[cfg(feature = "download")]
use regex::Regex;
#[cfg(feature = "download")]
use serde_json;
#[cfg(feature = "download")]
use std::path::Path;
#[cfg(feature = "download")]
use std::sync::OnceLock;
#[cfg(feature = "download")]
use tokio::fs::File;
#[cfg(feature = "download")]
use tokio::io::AsyncWriteExt;
#[cfg(feature = "download")]
use tokio::task::JoinSet;

#[cfg(feature = "ui")]
use crate::ui::ui::UIState;
#[cfg(not(feature = "ui"))]
use crate::core::types::UIState;

#[cfg(not(feature = "ui"))]
impl UIState {
    pub fn add_current_operation(&mut self, _name: String, _progress: f64, _size: u64) {}
    pub fn update_progress(&mut self, _name: &str, _progress: f64) {}
    pub fn remove_current_operation(&mut self, _name: &str, _url: Option<&str>) {}
    pub fn add_failed_operation(&mut self, _name: String, _info: FailedOperationInfo) {}
    pub fn add_processed_bytes(&mut self, _bytes: u64) {}
}

pub struct BunkrDownloader {
    #[cfg(feature = "download")]
    client: Client,
    #[cfg(feature = "download")]
    headers: header::HeaderMap,
    #[cfg(feature = "download")]
    album_files_regex: OnceLock<Regex>,
    #[cfg(feature = "download")]
    trailing_comma_regex: OnceLock<Regex>,
    #[cfg(feature = "download")]
    keys_regex: OnceLock<Regex>,
    #[cfg(feature = "download")]
    id_regex: OnceLock<Regex>,
    #[cfg(feature = "download")]
    orig_regex: OnceLock<Regex>,
}

#[cfg(not(feature = "download"))]
impl BunkrDownloader {
    pub async fn new() -> Result<Self> {
        Err(anyhow!("Download feature is not enabled."))
    }

    pub async fn get_files(&self, _album_url: &str) -> Result<Vec<AlbumFile>> {
        Err(anyhow!("Download feature is not enabled."))
    }

    pub async fn download_file(&self, _file: &AlbumFile, _output_dir: &str, _ui_state: Option<Arc<Mutex<UIState>>>) -> Result<()> {
        Err(anyhow!("Download feature is not enabled."))
    }

    pub async fn download_files(&self, _files: Vec<AlbumFile>, _output_dir: &str, _batch_size: usize, _ui_state: Option<Arc<Mutex<UIState>>>) -> Result<()> {
        Err(anyhow!("Download feature is not enabled."))
    }
}

#[cfg(feature = "download")]
impl BunkrDownloader {
    pub async fn new() -> Result<Self> {
        let client = Client::new();

        let mut headers = header::HeaderMap::new();
        headers.insert("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".parse()?);
        headers.insert("Accept", "application/json, text/plain, */*".parse()?);
        headers.insert("Accept-Language", "en-US,en;q=0.9".parse()?);
        headers.insert("Content-Type", "application/json".parse()?);
        headers.insert("Origin", "https://bunkr.su".parse()?);

        let album_files_regex = OnceLock::new();
        album_files_regex.get_or_init(|| Regex::new(r"window\.albumFiles\s*=\s*\[\s*([\s\S]*?)\s*\]\s*;").unwrap());

        let trailing_comma_regex = OnceLock::new();
        trailing_comma_regex.get_or_init(|| Regex::new(r",(\s*)([}\]])").unwrap());

        let keys_regex = OnceLock::new();
        keys_regex.get_or_init(|| Regex::new(r"(?m)^(\s*)(\w+)(\s*):").unwrap());

        let id_regex = OnceLock::new();
        id_regex.get_or_init(|| Regex::new(r#"data-file-id="(\d+)""#).unwrap());

        let orig_regex = OnceLock::new();
        orig_regex.get_or_init(|| Regex::new(r#"<h1 class="text-subs font-semibold text-base sm:text-lg truncate">([^<]+)</h1>"#).unwrap());

        Ok(Self {
            client,
            headers,
            album_files_regex,
            trailing_comma_regex,
            keys_regex,
            id_regex,
            orig_regex,
        })
    }

    pub async fn get_files(&self, album_url: &str) -> Result<Vec<AlbumFile>> {
        if album_url.contains("/a/") {
            self.get_album_files(album_url).await
        } else if album_url.contains("/f/") {
            let file = self.get_single_file(album_url).await?;
            Ok(vec![file])
        } else {
            Err(anyhow!("Unsupported URL: {}", album_url))
        }
    }

    async fn get_album_files(&self, album_url: &str) -> Result<Vec<AlbumFile>> {
        // Album URL
        // Ensure the URL has advanced=1
        let url = if album_url.contains("?") {
            format!("{}&advanced=1", album_url)
        } else {
            format!("{}?advanced=1", album_url)
        };

        let response = self.client.get(&url).send().await?;
        let html = response.text().await?;

        // Regex to extract the window.albumFiles array
        let re = self.album_files_regex.get().unwrap();

        let captures = re.captures(&html)
            .ok_or_else(|| anyhow!("Could not find albumFiles in the page"))?;

        let js_array = &captures[1];

        // Convert JavaScript object notation to JSON
        let json_str = self.js_to_json(js_array)?;

        // Parse the JS-like array using json5 (supports single quotes, trailing commas, etc.)
        let files: Vec<AlbumFile> = json5::from_str(&json_str)?;

        Ok(files)
    }

    async fn get_single_file(&self, file_url: &str) -> Result<AlbumFile> {
        // Individual file URL
        let response = self.client.get(file_url).send().await?;
        let html = response.text().await?;

        // Extract file id from <div id="fileTracker" data-file-id="...">
        let id_re = self.id_regex.get().unwrap();
        let id: i64 = id_re.captures(&html)
            .and_then(|c| c[1].parse().ok())
            .ok_or_else(|| anyhow!("Could not find file id"))?;

        // Extract original filename from <h1 class="text-subs font-semibold text-base sm:text-lg truncate">
        let orig_re = self.orig_regex.get().unwrap();
        let original = orig_re.captures(&html)
            .and_then(|c| Some(c[1].to_string()))
            .ok_or_else(|| anyhow!("Could not find file name"))?;

        // Create AlbumFile with extracted data, defaults for others
        let file = AlbumFile {
            id,
            name: original.clone(),
            original,
            slug: "".to_string(),
            file_type: "".to_string(),
            extension: "".to_string(),
            size: 0,
            timestamp: "".to_string(),
            thumbnail: "".to_string(),
            cdn_endpoint: "".to_string(),
        };

        Ok(file)
    }

    fn js_to_json(&self, js_str: &str) -> Result<String> {
        // Replace JavaScript object syntax with JSON
        let mut json = js_str.to_string();

        // Remove trailing commas before closing braces/brackets
        let re_trailing_comma = self.trailing_comma_regex.get().unwrap();
        json = re_trailing_comma.replace_all(&json, "$2").to_string();

        // Replace unquoted keys with quoted keys
        // This is a simple approach; for more complex cases, a proper JS parser might be needed
        let re_keys = self.keys_regex.get().unwrap();
        json = re_keys.replace_all(&json, "$1\"$2\"$3:").to_string();

        // Wrap in array brackets if not already
        let json = format!("[{}]", json);

        Ok(json)
    }

    pub async fn download_file(&self, file: &AlbumFile, output_dir: &str, ui_state: Option<Arc<Mutex<UIState>>>) -> Result<()> {
        Self::download_file_owned(
            self.client.clone(),
            self.headers.clone(),
            Self::owned_album_file(file),
            output_dir.to_string(),
            ui_state,
        ).await
    }

    fn owned_album_file(file: &AlbumFile) -> AlbumFile {
        AlbumFile {
            id: file.id,
            name: file.name.clone(),
            original: file.original.clone(),
            slug: file.slug.clone(),
            file_type: file.file_type.clone(),
            extension: file.extension.clone(),
            size: file.size,
            timestamp: file.timestamp.clone(),
            thumbnail: file.thumbnail.clone(),
            cdn_endpoint: file.cdn_endpoint.clone(),
        }
    }

    async fn download_file_owned(
        client: Client,
        headers: header::HeaderMap,
        file: AlbumFile,
        output_dir: String,
        ui_state: Option<Arc<Mutex<UIState>>>,
    ) -> Result<()> {
        // Post to the API to get the download URL
        let api_url = "https://apidl.bunkr.ru/api/_001_v2";
        let body = serde_json::json!({ "id": file.id.to_string() });

        let response = client.post(api_url).headers(headers).json(&body).send().await?;
        let response_text = response.text().await?;

        if !response_text.trim().starts_with('{') {
            return Err(anyhow!("API returned non-JSON response: {}", response_text));
        }

        let download_resp: DownloadResponse = serde_json::from_str(&response_text)?;

        if !download_resp.encrypted {
            return Err(anyhow!("Download URL is not encrypted"));
        }

        // Decode the URL
        let decoded_url = BunkrDownloader::decrypt_url(&download_resp.url, download_resp.timestamp)?;

        // Append the name parameter as per JS
        let separator = if decoded_url.contains('?') { '&' } else { '?' };
        let encoded_name = urlencoding::encode(&file.original);
        let full_url = format!("{}{}n={}", decoded_url, separator, encoded_name);

        // Download the file
        let mut download_headers = header::HeaderMap::new();
        download_headers.insert("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:146.0) Gecko/20100101 Firefox/146.0".parse()?);
        download_headers.insert("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8".parse()?);
        download_headers.insert("Accept-Language", "en-US,en;q=0.5".parse()?);
        download_headers.insert("Referer", "https://get.bunkrr.su/".parse()?);

        let response = client.get(&full_url).headers(download_headers).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!("Failed to download file: {}", response.status()));
        }

        let total_size = response.content_length().unwrap_or(file.size as u64);
        let mut downloaded = 0u64;
        let file_path = Path::new(&output_dir).join(&file.original);
        let mut file_handle = File::create(&file_path).await?;

        let mut response = response;
        while let Some(chunk) = response.chunk().await? {
            file_handle.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;

            if let Some(ref state) = ui_state {
                let mut state = state.lock().unwrap();
                let progress = if total_size > 0 { (downloaded as f64 / total_size as f64).min(1.0) } else { 0.0 };
                state.update_progress(&file.original, progress);
                state.add_processed_bytes(chunk.len() as u64);
            }
        }

        Ok(())
    }

    pub async fn download_files(&self, files: Vec<AlbumFile>, output_dir: &str, batch_size: usize, ui_state: Option<Arc<Mutex<UIState>>>) -> Result<()> {
        let batch_size = batch_size.max(1);
        let output_dir = output_dir.to_string();
        let client = self.client.clone();
        let headers = self.headers.clone();

        let mut files_iter = files.into_iter();
        let mut join_set = JoinSet::new();

        let mut spawn_next = |join_set: &mut JoinSet<(AlbumFile, Result<()>)>| {
            let Some(file) = files_iter.next() else {
                return;
            };

            let client = client.clone();
            let headers = headers.clone();
            let output_dir = output_dir.clone();
            let ui_state = ui_state.clone();

            join_set.spawn(async move {
                if let Some(ref state) = ui_state {
                    let mut state = state.lock().unwrap();
                    state.add_current_operation(file.original.clone(), 0.0, file.size as u64);
                }

                let file_for_result = BunkrDownloader::owned_album_file(&file);
                let result = BunkrDownloader::download_file_owned(client, headers, file, output_dir, ui_state.clone()).await;
                (file_for_result, result)
            });
        };

        for _ in 0..batch_size {
            spawn_next(&mut join_set);
        }

        while let Some(result) = join_set.join_next().await {
            if let Ok((file, result)) = result {
                match result {
                    Ok(_) => {
                        if let Some(ref state) = ui_state {
                            let mut state = state.lock().unwrap();
                            state.remove_current_operation(&file.original, None);
                        }
                    }
                    Err(e) => {
                        if let Some(ref state) = ui_state {
                            let mut state = state.lock().unwrap();
                            let info = FailedOperationInfo {
                                path: file.original.clone(),
                                error: e.to_string(),
                                file_size: file.size as u64,
                                status_code: None,
                            };
                            state.add_failed_operation(file.original.clone(), info);
                        }
                    }
                }
            }

            spawn_next(&mut join_set);
        }

        Ok(())
    }

    fn decrypt_url(encrypted_base64: &str, timestamp: i64) -> Result<String> {
        // Calculate the key as per the JavaScript
        let divisor = 3600.0;
        let suffix = ((timestamp as f64) / divisor).floor() as i64;
        let key = format!("SECRET_KEY_{}", suffix);

        // Base64 decode
        let bytes = general_purpose::STANDARD.decode(encrypted_base64)?;

        // XOR decrypt with key
        let key_bytes = key.as_bytes();
        let mut output = Vec::with_capacity(bytes.len());
        for (i, &b) in bytes.iter().enumerate() {
            output.push(b ^ key_bytes[i % key_bytes.len()]);
        }

        // Decode as UTF-8
        let decoded = String::from_utf8(output)?;
        Ok(decoded)
    }
}