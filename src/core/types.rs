use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct UserPermissions {
    pub user: bool,
    pub vip: bool,
    pub vvip: bool,
    pub moderator: bool,
    pub admin: bool,
    pub superadmin: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
#[allow(non_snake_case)]
pub struct VerifyResponse {
    pub success: bool,
    pub username: Option<String>,
    pub permissions: Option<UserPermissions>,
    pub group: Option<String>,
    pub retentionPeriods: Option<Vec<i64>>,
    pub defaultRetentionPeriod: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct NodeResponse {
    pub success: bool,
    pub url: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct UploadResponse {
    pub success: bool,
    pub files: Option<Vec<UploadedFile>>,
}

#[derive(Debug, Deserialize)]
pub struct UploadedFile {
    pub url: String,
}

#[derive(Clone, Debug)]
pub struct FailedOperationInfo {
    pub path: String,
    pub error: String,
    pub file_size: u64,
    pub status_code: Option<u16>,
}

#[cfg(not(feature = "ui"))]
#[derive(Clone)]
pub struct UIState;

#[derive(Debug, Deserialize)]
pub struct Album {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct AlbumFile {
    pub id: i64,
    pub name: String,
    pub original: String,
    pub slug: String,
    #[serde(rename = "type")]
    pub file_type: String,
    pub extension: String,
    pub size: i64,
    pub timestamp: String,
    pub thumbnail: String,
    #[serde(rename = "cdnEndpoint")]
    pub cdn_endpoint: String,
}

#[derive(Debug, Deserialize)]
pub struct DownloadResponse {
    pub encrypted: bool,
    pub timestamp: i64,
    pub url: String,
}
