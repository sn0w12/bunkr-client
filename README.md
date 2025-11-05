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

## Usage

### First Time Setup

Save your API token securely:

```bash
bunkr_uploader save-token YOUR_API_TOKEN
```

### Upload Files

Upload files to an existing album by name or id:

```bash
bunkr_uploader --album-id ALBUM_ID file1.jpg file2.png
bunkr_uploader --album-name "My Album" file1.jpg file2.png
```

Upload a directory:

```bash
bunkr_uploader /path/to/directory
```

### Create Album

```bash
bunkr_uploader create-album "Album Name" --description "Description"
```

### Configuration

View current config:

```bash
bunkr_uploader config get
```

Set default batch size:

```bash
bunkr_uploader config set default_batch_size 5
```

## Options

-   `--token`: Provide API token (alternative to saving)
-   `--album-id`: Upload to specific album ID
-   `--album-name`: Upload to album by name
-   `--batch-size`: Number of files to upload concurrently
-   `--help`: Show help

## License

See LICENSE file.
