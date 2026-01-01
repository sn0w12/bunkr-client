#[cfg(feature = "cli")]
use clap::{Parser, Subcommand};
#[cfg(feature = "cli")]
use bunkr_client::BunkrUploader;
#[cfg(feature = "ui")]
use bunkr_client::ui::ui::{UIState, start_ui, stop_ui};
#[cfg(feature = "ui")]
use bunkr_client::ui::ui::OperationStatus;
#[cfg(feature = "cli")]
#[cfg(not(feature = "ui"))]
use bunkr_client::core::types::UIState;
use anyhow::Result;
use keyring::Entry;
use std::{path::Path, sync::{Arc, Mutex}, io::Write, fs::OpenOptions};


pub fn get_styles() -> clap::builder::Styles {
    clap::builder::Styles::styled()
        .usage(
            anstyle::Style::new()
                .bold()
                .underline()
        )
        .header(
            anstyle::Style::new()
                .bold()
                .underline()
        )
        .literal(
            anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::BrightGreen))),
        )
        .invalid(
            anstyle::Style::new()
                .bold()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Red))),
        )
        .error(
            anstyle::Style::new()
                .bold()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Red))),
        )
        .valid(
            anstyle::Style::new()
                .bold()
                .underline()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Green))),
        )
        .placeholder(
            anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::White))),
        )
}


#[cfg(feature = "cli")]
#[derive(Parser)]
#[command(name = "bunkr_client", about = "CLI tool for uploading files to Bunkr.cr", styles = get_styles())]
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

#[cfg(feature = "cli")]
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
    /// Download files from an album
    Download {
        album_url: String,
        #[arg(short, long)]
        output_dir: Option<String>,
    },
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[cfg(feature = "cli")]
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

#[cfg(feature = "cli")]
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

#[cfg(feature = "cli")]
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = bunkr_client::Config::load()?;
    let batch_size = cli.batch_size.or_else(|| config.default_batch_size).unwrap_or(1);
    let album_id = cli.album_id.or_else(|| config.default_album_id.clone());
    let album_name = cli.album_name.or_else(|| config.default_album_name.clone());

    match cli.command {
        Some(Commands::SaveToken { token: save_token }) => {
            let entry = Entry::new("bunkr_client", "api_token")?;
            entry.set_password(&save_token)?;
            println!("Token saved securely.");
        }
        Some(Commands::CreateAlbum { name, description, download, public }) => {
            let token = bunkr_client::core::utils::get_token(cli.token)?;
            let uploader = BunkrUploader::new(token).await?;
            let id = uploader.create_album(name, description, download, public).await?;
            println!("Album created with ID: {}", id);
        }
        Some(Commands::Download { album_url, output_dir }) => {
            let downloader = bunkr_client::BunkrDownloader::new().await?;
            let files = downloader.get_files(&album_url).await?;

            let output_dir = output_dir.unwrap_or_else(|| ".".to_string());
            std::fs::create_dir_all(&output_dir)?;

            let total_bytes: u64 = files.iter().map(|f| f.size as u64).sum();

            #[cfg(feature = "ui")]
            let ui_state = Some(Arc::new(Mutex::new(UIState::new(files.len(), None, total_bytes))));
            #[cfg(not(feature = "ui"))]
            let ui_state: Option<Arc<Mutex<UIState>>> = None;
            #[cfg(feature = "ui")]
            let (ui_handle, running) = start_ui(ui_state.as_ref().unwrap().clone());

            let ui_state_for_download = ui_state.as_ref().map(|arc| Arc::clone(arc));
            downloader.download_files(files, &output_dir, ui_state_for_download).await?;

            // Print failed operations
            #[cfg(feature = "ui")]
            {
                stop_ui(ui_handle, running);

                if let Some(ref state) = ui_state {
                    let state = state.lock().unwrap();
                    let failed: Vec<_> = state.all_operations.iter()
                        .filter_map(|(name, status)| {
                            if let OperationStatus::Failed(info) = status {
                                Some((name.clone(), info.clone()))
                            } else {
                                None
                            }
                        })
                        .collect();
                    if !failed.is_empty() {
                        println!("Failed downloads:");
                        for (name, info) in failed {
                            println!("  {}: {} (size: {}, status: {:?})", name, info.error, info.file_size, info.status_code);
                        }
                    } else {
                        println!("All downloads completed successfully.");
                    }
                }
            }
            #[cfg(not(feature = "ui"))]
            println!("Download completed. Check for any errors above.");
        }
        Some(Commands::Config { action }) => {
            let mut config = bunkr_client::Config::load()?;
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

            let token = bunkr_client::core::utils::get_token(cli.token)?;

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

            let (_urls, failures) = uploader.upload_files(all_files, album_id.as_deref(), batch_size, ui_state, Some(&config)).await?;

            #[cfg(feature = "ui")]
            {
                stop_ui(ui_handle, running);
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

#[cfg(not(feature = "cli"))]
fn main() {
    eprintln!("CLI feature not enabled. This binary requires the 'cli' feature.");
    std::process::exit(1);
}
