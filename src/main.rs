mod core;
mod config;
#[cfg(feature = "ui")]
mod ui;
mod preprocess;

use clap::{Parser, Subcommand};
use core::uploader::BunkrUploader;
#[cfg(feature = "ui")]
use crate::ui::ui::{UIState, start_ui};
#[cfg(not(feature = "ui"))]
use crate::core::uploader::UIState;
use anyhow::Result;
use keyring::Entry;
use std::{path::Path, sync::{Arc, Mutex}, io::Write, fs::OpenOptions};
#[cfg(feature = "ui")]
use crossterm::{cursor, terminal, ExecutableCommand};
#[cfg(feature = "ui")]
use std::io;

#[derive(Parser)]
#[command(name = "bunkr_uploader", about = "CLI tool for uploading files to Bunkr.cr")]
struct Cli {
    #[arg(short, long)]
    token: Option<String>,

    #[arg(short = 'a', long)]
    album_id: Option<String>,

    #[arg(short = 'n', long)]
    album_name: Option<String>,

    #[arg(short = 'b', long)]
    batch_size: Option<usize>,

    paths: Vec<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Save the API token securely
    SaveToken { token: String },
    /// Create a new album
    CreateAlbum {
        name: String,
        #[arg(short, long)]
        description: Option<String>,
        #[arg(short, long, default_value = "true")]
        download: bool,
        #[arg(short, long, default_value = "true")]
        public: bool,
    },
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Get configuration value(s)
    Get {
        /// Specific key to get, if omitted get all
        key: Option<String>,
    },
    /// Set configuration value
    Set {
        key: String,
        value: String,
    },
}

fn collect_all_files(paths: &[String]) -> Result<Vec<String>> {
    let mut files = vec![];
    for path in paths {
        let p = Path::new(path);
        if p.is_file() {
            files.push(path.clone());
        } else if p.is_dir() {
            let dir_files = std::fs::read_dir(p)?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .map(|e| e.path().to_string_lossy().to_string())
                .collect::<Vec<_>>();
            files.extend(dir_files);
        } else {
            return Err(anyhow::anyhow!("Invalid path: {}", path));
        }
    }
    Ok(files)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = config::config::Config::load()?;
    let batch_size = cli.batch_size.or_else(|| config.default_batch_size).unwrap_or(1);
    let album_id = cli.album_id.or_else(|| config.default_album_id.clone());
    let album_name = cli.album_name.or_else(|| config.default_album_name.clone());

    match cli.command {
        Some(Commands::SaveToken { token: save_token }) => {
            let entry = Entry::new("bunkr_uploader", "api_token")?;
            entry.set_password(&save_token)?;
            println!("Token saved securely.");
        }
        Some(Commands::CreateAlbum { name, description, download, public }) => {
            let token = core::utils::get_token(cli.token)?;
            let uploader = BunkrUploader::new(token).await?;
            let id = uploader.create_album(name, description, download, public).await?;
            println!("Album created with ID: {}", id);
        }
        Some(Commands::Config { action }) => {
            let mut config = config::config::Config::load()?;
            match action {
                ConfigAction::Get { key } => {
                    if let Some(k) = key {
                        let value = config.get_value(&k);
                        println!("{}", value);
                    } else {
                        config.print_all();
                    }
                }
                ConfigAction::Set { key, value } => {
                    config.set_value(&key, &value)?;
                    config.save()?;
                    println!("Config updated.");
                }
            }
        }
        None => {
            let all_files = collect_all_files(&cli.paths)?;
            if all_files.is_empty() {
                return Err(anyhow::anyhow!("No files to upload."));
            }

            let total_bytes: u64 = all_files.iter()
                .filter_map(|f| std::fs::metadata(f).ok().map(|m| m.len()))
                .sum();

            let token = core::utils::get_token(cli.token)?;

            let uploader = BunkrUploader::new(token).await?;

            let album_id = if let Some(name) = album_name {
                if let Some(id) = uploader.get_album_by_name(&name).await? {
                    Some(id.to_string())
                } else {
                    return Err(anyhow::anyhow!("Album '{}' not found", name));
                }
            } else {
                album_id
            };

            #[cfg(feature = "ui")]
            let ui_state = Some(Arc::new(Mutex::new(UIState::new(all_files.len(), album_id.clone(), total_bytes))));
            #[cfg(not(feature = "ui"))]
            let ui_state: Option<Arc<Mutex<UIState>>> = None;
            #[cfg(feature = "ui")]
            let (ui_handle, running) = start_ui(ui_state.as_ref().unwrap().clone());

            let (_urls, failures) = uploader.upload_files(all_files, album_id.as_deref(), batch_size, ui_state, &config).await?;

            #[cfg(feature = "ui")]
            {
                // Stop the UI
                running.store(false, std::sync::atomic::Ordering::Relaxed);
                ui_handle.join().unwrap();

                // Clear the UI and print final results
                let mut stdout = io::stdout();
                stdout.execute(terminal::Clear(terminal::ClearType::All)).unwrap();
                stdout.execute(cursor::MoveTo(0, 0)).unwrap();
            }

            // Write the failed uploads to a file
            if !failures.is_empty() {
                let mut failed_file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("failed_uploads.txt")?;
                for failure in &failures {
                    writeln!(failed_file, "File: {}, Error: {}, Size: {}, Status: {:?}", failure.path, failure.error, failure.file_size, failure.status_code)?;
                }
            }
        }
    }

    Ok(())
}
