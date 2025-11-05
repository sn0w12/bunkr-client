use crate::{config::bunkr_config::BunkrConfig, config::config::Config, preprocess::preprocess::cleanup_preprocess, core::types::*, core::utils::parse_size};
#[cfg(feature = "ui")]
use crate::ui::ui::{UIState, FailedUploadInfo};
use anyhow::{Result, anyhow};
use mime_guess::from_path;
use reqwest::{Client, multipart};
use serde_json::json;
use std::{path::Path, fs::File, io::Read, sync::{Arc, Mutex}};
use futures::future::join_all;
use uuid::Uuid;

pub struct BunkrUploader {
    client: Client,
    headers: reqwest::header::HeaderMap,
    upload_url: String,
    max_file_size: u64,
    chunk_size: u64,
}

impl BunkrUploader {
    pub async fn new(token: String) -> Result<Self> {
        let client = Client::new();

        // Verify token
        let response = client
            .post("https://dash.bunkr.cr/api/tokens/verify")
            .form(&[("token", token.clone())])
            .send()
            .await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(anyhow!("Token verification failed with status {}: {}", status, text));
        }
        let verify: VerifyResponse = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to parse token verification response: {}", e);
                eprintln!("Response: {}", text);
                return Err(anyhow!("JSON parsing error: {}", e));
            }
        };
        if !verify.success {
            return Err(anyhow!("Invalid API token"));
        }

        // Fetch configuration
        let response = client
            .get("https://dash.bunkr.cr/api/check")
            .header("token", &token)
            .send()
            .await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(anyhow!("Config fetch failed with status {}: {}", status, text));
        }
        let config: BunkrConfig = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to parse config response: {}", e);
                eprintln!("Response: {}", text);
                return Err(anyhow!("JSON parsing error: {}", e));
            }
        };

        let response = client
            .get("https://dash.bunkr.cr/api/node")
            .header("token", &token)
            .send()
            .await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(anyhow!("Node fetch failed with status {}: {}", status, text));
        }
        let node: NodeResponse = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to parse node response: {}", e);
                eprintln!("Response: {}", text);
                return Err(anyhow!("JSON parsing error: {}", e));
            }
        };

        let max_file_size = parse_size(&config.maxSize)?;
        let chunk_size = parse_size(&config.chunkSize.default)?;

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("token", token.parse()?);

        Ok(Self {
            client,
            headers,
            upload_url: node.url,
            max_file_size,
            chunk_size,
        })
    }

    pub async fn upload_file(&self, path: &str, album_id: Option<&str>, ui_state: Option<Arc<Mutex<UIState>>>, config: &Config) -> Result<Option<String>> {
        let p = Path::new(path);
        if !p.exists() {
            let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            #[cfg(feature = "ui")]
            if let Some(ui_state) = &ui_state {
                ui_state.lock().unwrap().add_failed(path.to_string(), FailedUploadInfo {
                    error: format!("File not found: {}", path),
                    file_size: size,
                    status_code: None,
                });
            }
            return Ok(None);
        }

        let preprocess_result = crate::preprocess::preprocess::preprocess_file(path, self.max_file_size, config)?;
        let mut urls = vec![];
        for file_path in &preprocess_result.files_to_upload {
            let p = Path::new(file_path);
            if !p.exists() {
                continue;
            }
            let metadata = p.metadata()?;
            let size = metadata.len();
            let mime = from_path(p).first_or_octet_stream();
            let url = if size <= self.chunk_size {
                self.upload_single_file(p, mime.essence_str(), album_id, ui_state.clone(), size).await?
            } else {
                self.upload_chunked_file(p, mime.essence_str(), album_id, ui_state.clone(), size).await?
            };
            if let Some(u) = url {
                urls.push(u);
            }
        }
        // Cleanup after upload
        cleanup_preprocess(&preprocess_result.preprocess_id, path, &preprocess_result.files_to_upload);
        Ok(Some(urls.join(",")))
    }

    async fn upload_single_file(
        &self,
        path: &Path,
        mime: &str,
        album_id: Option<&str>,
        ui_state: Option<Arc<Mutex<UIState>>>,
        file_size: u64,
    ) -> Result<Option<String>> {
        let file_name = path.file_name().unwrap().to_string_lossy().to_string();
        let mut file = File::open(path)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        let size = buf.len() as u64;

        #[cfg(feature = "ui")]
        if let Some(ui_state) = &ui_state {
            let mut state = ui_state.lock().unwrap();
            state.add_current(file_name.clone(), 0.0);
        }

        let part = multipart::Part::bytes(buf).file_name(file_name.clone()).mime_str(mime)?;
        let form = multipart::Form::new().part("files[]", part);

        let mut headers = self.headers.clone();
        if let Some(album_id) = album_id {
            headers.insert("albumid", reqwest::header::HeaderValue::from_str(album_id)?);
        }

        let response = self
            .client
            .post(&self.upload_url)
            .headers(headers)
            .multipart(form)
            .send()
            .await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            #[cfg(feature = "ui")]
            if let Some(ui_state) = &ui_state {
                ui_state.lock().unwrap().add_failed(path.to_string_lossy().to_string(), FailedUploadInfo {
                    error: format!("Upload request failed with status {}: {}", status, text),
                    file_size,
                    status_code: Some(status.as_u16()),
                });
            }
            return Ok(None);
        }
        let res: UploadResponse = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(e) => {
                #[cfg(feature = "ui")]
                if let Some(ui_state) = &ui_state {
                    ui_state.lock().unwrap().add_failed(path.to_string_lossy().to_string(), FailedUploadInfo {
                        error: format!("Failed to parse upload response: {}", e),
                        file_size,
                        status_code: None,
                    });
                }
                return Ok(None);
            }
        };

        if !res.success {
            #[cfg(feature = "ui")]
            if let Some(ui_state) = &ui_state {
                ui_state.lock().unwrap().add_failed(path.to_string_lossy().to_string(), FailedUploadInfo {
                    error: format!("Upload failed: server returned success=false"),
                    file_size,
                    status_code: None,
                });
            }
            return Ok(None);
        }

        {
            #[cfg(feature = "ui")]
            if let Some(ui_state) = &ui_state {
                let mut state = ui_state.lock().unwrap();
                state.update_progress(&file_name, 1.0);
                state.add_uploaded_bytes(size);
                state.remove_current(&file_name);
            }
        }

        Ok(res.files.and_then(|f| f.first().map(|x| x.url.clone())))
    }

    async fn upload_chunked_file(
        &self,
        path: &Path,
        mime: &str,
        album_id: Option<&str>,
        ui_state: Option<Arc<Mutex<UIState>>>,
        file_size: u64,
    ) -> Result<Option<String>> {
        let total_size = path.metadata()?.len();
        let total_chunks = (total_size as f64 / self.chunk_size as f64).ceil() as u64;
        let file_name = path.file_name().unwrap().to_string_lossy().to_string();

        #[cfg(feature = "ui")]
        if let Some(ui_state) = &ui_state {
            let mut state = ui_state.lock().unwrap();
            state.add_current(file_name.clone(), 0.0);
        }

        let uuid = Uuid::new_v4();
        let mut file = File::open(path)?;

        for i in 0..total_chunks {
            let mut buf = vec![0u8; self.chunk_size as usize];
            let bytes_read = file.read(&mut buf)?;
            buf.truncate(bytes_read);

            let part = multipart::Part::bytes(buf)
                .file_name(file_name.clone())
                .mime_str("application/octet-stream")?;

            let form = multipart::Form::new()
                .text("dzuuid", uuid.to_string())
                .text("dzchunkindex", i.to_string())
                .part("files[]", part);

            let response = self.client
                .post(&self.upload_url)
                .headers(self.headers.clone())
                .multipart(form)
                .send()
                .await?;
            let status = response.status();
            if !status.is_success() {
                let text = response.text().await?;
                #[cfg(feature = "ui")]
                if let Some(ui_state) = &ui_state {
                    ui_state.lock().unwrap().add_failed(path.to_string_lossy().to_string(), FailedUploadInfo {
                        error: format!("Chunk {} upload failed with status {}: {}", i, status, text),
                        file_size,
                        status_code: Some(status.as_u16()),
                    });
                }
                return Ok(None);
            }

            {
                let progress = (i + 1) as f64 / total_chunks as f64;
                #[cfg(feature = "ui")]
                if let Some(ui_state) = &ui_state {
                    let mut state = ui_state.lock().unwrap();
                    state.update_progress(&file_name, progress);
                    state.add_uploaded_bytes(bytes_read as u64);
                }
            }
        }

        {
            #[cfg(feature = "ui")]
            if let Some(ui_state) = &ui_state {
                let mut state = ui_state.lock().unwrap();
                state.remove_current(&file_name);
            }
        }

        if let Some(album_id) = album_id {
            let finish_url = format!("{}/finishchunks", self.upload_url);
            let original = file_name;
            let body = json!({
                "files": [{
                    "uuid": uuid.to_string(),
                    "original": original,
                    "type": mime,
                    "albumid": album_id.parse::<i64>().unwrap_or(0),
                    "filelength": null,
                    "age": null,
                }]
            });
            let response = self.client
                .post(&finish_url)
                .headers(self.headers.clone())
                .json(&body)
                .send()
                .await?;
            let status = response.status();
            let text = response.text().await?;
            if !status.is_success() {
                #[cfg(feature = "ui")]
                if let Some(ui_state) = &ui_state {
                    ui_state.lock().unwrap().add_failed(path.to_string_lossy().to_string(), FailedUploadInfo {
                        error: format!("Finish chunks request failed with status {}: {}", status, text),
                        file_size,
                        status_code: Some(status.as_u16()),
                    });
                }
                return Ok(None);
            }
            let res: UploadResponse = match serde_json::from_str(&text) {
                Ok(r) => r,
                Err(e) => {
                    #[cfg(feature = "ui")]
                    if let Some(ui_state) = &ui_state {
                        ui_state.lock().unwrap().add_failed(path.to_string_lossy().to_string(), FailedUploadInfo {
                            error: format!("Failed to parse finish chunks response: {}", e),
                            file_size,
                            status_code: None,
                        });
                    }
                    return Ok(None);
                }
            };
            if !res.success {
                #[cfg(feature = "ui")]
                if let Some(ui_state) = &ui_state {
                    ui_state.lock().unwrap().add_failed(path.to_string_lossy().to_string(), FailedUploadInfo {
                        error: format!("Finish chunks failed: server returned success=false"),
                        file_size,
                        status_code: None,
                    });
                }
                return Ok(None);
            }
            Ok(res.files.and_then(|f| f.first().map(|x| x.url.clone())))
        } else {
            Ok(None)
        }
    }

    pub async fn upload_files(
        &self,
        files: Vec<String>,
        album_id: Option<&str>,
        batch_size: usize,
        ui_state: Option<Arc<Mutex<UIState>>>,
        config: &Config,
    ) -> Result<Vec<String>> {
        let mut results = vec![];

        // Clone the necessary data to move into the async tasks
        let client = self.client.clone();
        let headers = self.headers.clone();
        let upload_url = self.upload_url.clone();
        let max_file_size = self.max_file_size;
        let chunk_size = self.chunk_size;
        let album_id_owned = album_id.map(|s| s.to_string());
        let config_owned = config.clone();

        for chunk in files.chunks(batch_size) {
            let mut handles = vec![];
            let config_owned = config_owned.clone();

            for f in chunk {
                let f = f.clone();
                let client = client.clone();
                let headers = headers.clone();
                let upload_url = upload_url.clone();
                let album_id_owned = album_id_owned.clone();
                let ui_state = ui_state.clone();
                let config_owned = config_owned.clone();

                handles.push(tokio::spawn(async move {
                    let uploader = BunkrUploader {
                        client,
                        headers,
                        upload_url,
                        max_file_size,
                        chunk_size,
                    };
                    let url = uploader.upload_file(&f, album_id_owned.as_deref(), ui_state.clone(), &config_owned).await.unwrap_or(None);
                    url
                }));
            }

            let chunk_results = join_all(handles).await;
            results.extend(chunk_results.into_iter().filter_map(|r| r.ok().flatten()));
        }

        Ok(results)
    }

    pub async fn get_albums(&self) -> Result<Vec<Album>> {
        #[derive(serde::Deserialize)]
        struct AlbumsResponse {
            albums: Vec<Album>,
        }
        let response = self
            .client
            .get("https://dash.bunkr.cr/api/albums")
            .headers(self.headers.clone())
            .send()
            .await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(anyhow!("Albums fetch failed with status {}: {}", status, text));
        }
        let res: AlbumsResponse = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to parse albums response: {}", e);
                eprintln!("Response: {}", text);
                return Err(anyhow!("JSON parsing error: {}", e));
            }
        };
        Ok(res.albums)
    }

    pub async fn get_album_by_name(&self, album_name: &str) -> Result<Option<i64>> {
        let albums = self.get_albums().await?;
        for album in albums {
            if album.name.to_lowercase() == album_name.to_lowercase() {
                return Ok(Some(album.id));
            }
        }
        Ok(None)
    }

    pub async fn create_album(&self, name: String, description: Option<String>, download: bool, public: bool) -> Result<i64> {
        let body = json!({
            "name": name,
            "description": description.unwrap_or_default(),
            "download": download,
            "public": public,
        });

        let response = self.client
            .post("https://dash.bunkr.cr/api/albums")
            .headers(self.headers.clone())
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(anyhow!("Create album failed with status {}: {}", status, text));
        }

        let res: serde_json::Value = serde_json::from_str(&text)?;
        if res["success"] == true {
            Ok(res["id"].as_i64().unwrap())
        } else {
            Err(anyhow!("Create album failed: success=false"))
        }
    }
}
