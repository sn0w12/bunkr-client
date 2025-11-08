use anyhow::Result;
use keyring::Entry;

pub fn parse_size(size_str: &str) -> Result<u64> {
    let s = size_str.trim().to_uppercase();
    if s.ends_with("GB") {
        Ok(s.trim_end_matches("GB").parse::<u64>()? * 1024 * 1024 * 1024)
    } else if s.ends_with("MB") {
        Ok(s.trim_end_matches("MB").parse::<u64>()? * 1024 * 1024)
    } else if s.ends_with("KB") {
        Ok(s.trim_end_matches("KB").parse::<u64>()? * 1024)
    } else if s.ends_with("B") {
        Ok(s.trim_end_matches("B").parse::<u64>()?)
    } else {
        Err(anyhow::anyhow!("Invalid size format: {}", size_str))
    }
}

pub fn get_token(cli_token: Option<String>) -> Result<String> {
    if let Some(t) = cli_token {
        Ok(t)
    } else {
        let entry = Entry::new("bunkr_client", "api_token")?;
        entry.get_password().map_err(|_| anyhow::anyhow!("No token provided and none saved. Use --token or save one with save-token command."))
    }
}
