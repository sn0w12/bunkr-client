use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
pub struct ChunkSizeConfig {
    pub max: String,
    pub default: String,
    pub timeout: i64,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
pub struct FileIdentifierConfig {
    pub min: i64,
    pub max: i64,
    pub default: i64,
    pub force: bool,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
#[allow(non_snake_case)]
pub struct StripTagsConfig {
    pub default: bool,
    pub video: bool,
    pub force: bool,
    pub blacklistExtensions: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
#[allow(non_snake_case)]
pub struct BunkrConfig {
    pub maintenance: bool,
    pub private: bool,
    pub enableUserAccounts: bool,
    pub maxSize: String,
    pub chunkSize: ChunkSizeConfig,
    pub fileIdentifierLength: FileIdentifierConfig,
    pub stripTags: StripTagsConfig,
    pub temporaryUploadAges: Vec<i64>,
    pub defaultTemporaryUploadAge: i64,
}