# bunkr-uploader

A command-line tool for uploading files to [Bunkr.cr](https://bunkr.cr).

## Features

-   Upload single files or directories
-   Create and manage albums
-   Batch uploading with configurable batch size
-   Optional TUI for progress tracking
-   Video preprocessing support
-   Secure token storage

## Installation

### From Source

```bash
git clone https://github.com/sn0w12/bunkr-uploader.git
cd bunkr-uploader
cargo install --path .
```

### As a Library

Add to your `Cargo.toml`:

```toml
[dependencies]
bunkr-client = "0.1.0"
```

To use without CLI features:

```toml
[dependencies]
bunkr-client = { version = "0.1.0", default-features = false }
```

## Usage

### As a Library

```rust
use bunkr_client::{BunkrUploader, Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config {
        default_batch_size: Some(5),
        default_album_id: None,
        default_album_name: Some("My Album".to_string()),
        preprocess_videos: Some(true),
    };
    // Or use default: let config = Config::default();

    let uploader = BunkrUploader::new("your_api_token".to_string()).await?;

    // Upload files
    let files = vec!["file1.jpg".to_string(), "file2.png".to_string()];
    let (urls, failures) = uploader.upload_files(files, None, 1, None, Some(&config)).await?;

    Ok(())
}
```

### First Time Setup (CLI)

Save your API token securely:

```bash
bunkr-client save-token YOUR_API_TOKEN
```

### Upload Files

Upload files to an existing album by name or id:

```bash
bunkr-client --album-id ALBUM_ID file1.jpg file2.png
bunkr-client --album-name "My Album" file1.jpg file2.png
```

Upload a directory:

```bash
bunkr-client /path/to/directory
```

### Create Album

```bash
bunkr-client create-album "Album Name" --description "Description"
```

### Configuration

View current config:

```bash
bunkr-client config get
```

Set default batch size:

```bash
bunkr-client config set default_batch_size 5
```

## Options

-   `--token`: Provide API token (alternative to saving)
-   `--album-id`: Upload to specific album ID
-   `--album-name`: Upload to album by name
-   `--batch-size`: Number of files to upload concurrently
-   `--help`: Show help

## License

See LICENSE file.
