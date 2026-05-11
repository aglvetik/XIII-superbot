use crate::state::GoogleFormRow;
use chrono::Utc;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const SHEETS_READONLY_SCOPE: &str = "https://www.googleapis.com/auth/spreadsheets.readonly";
const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const SHEETS_API_BASE: &str = "https://sheets.googleapis.com/v4/spreadsheets";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoogleSheetsPollConfig {
    pub credentials_file: PathBuf,
    pub sheet_id: String,
    pub sheet_name: String,
    pub start_row: i64,
    pub end_column: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoogleSheetsReadPlan {
    pub range: String,
    pub token_uri_status: &'static str,
    pub credentials_status: &'static str,
    pub sheet_id_status: &'static str,
}

#[derive(Debug, Clone)]
pub struct GoogleSheetsReadonlyClient {
    http: Client,
}

#[derive(Debug, Deserialize)]
struct ServiceAccountKey {
    client_email: String,
    private_key: String,
    #[serde(default)]
    token_uri: Option<String>,
}

#[derive(Debug, Serialize)]
struct JwtClaims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    exp: i64,
    iat: i64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct ValuesResponse {
    #[serde(default)]
    values: Vec<Vec<serde_json::Value>>,
}

impl GoogleSheetsReadonlyClient {
    pub fn new() -> Self {
        Self {
            http: Client::new(),
        }
    }

    pub async fn fetch_rows(
        &self,
        config: &GoogleSheetsPollConfig,
    ) -> Result<Vec<GoogleFormRow>, String> {
        let key = read_service_account_key(&config.credentials_file)?;
        let token_uri = key.token_uri.as_deref().unwrap_or(DEFAULT_TOKEN_URI);
        let token = self.oauth_access_token(&key, token_uri).await?;
        self.fetch_sheet_values(config, &token).await
    }

    async fn oauth_access_token(
        &self,
        key: &ServiceAccountKey,
        token_uri: &str,
    ) -> Result<String, String> {
        let now = Utc::now().timestamp();
        let claims = JwtClaims {
            iss: &key.client_email,
            scope: SHEETS_READONLY_SCOPE,
            aud: token_uri,
            iat: now,
            exp: now + 3600,
        };
        let assertion = encode(
            &Header::new(Algorithm::RS256),
            &claims,
            &EncodingKey::from_rsa_pem(key.private_key.as_bytes()).map_err(|err| {
                format!("failed to parse Google service account private key: {err}")
            })?,
        )
        .map_err(|err| format!("failed to sign Google service account JWT: {err}"))?;

        let response = self
            .http
            .post(token_uri)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", assertion.as_str()),
            ])
            .send()
            .await
            .map_err(|err| format!("Google OAuth token request failed: {err}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "Google OAuth token request failed with status {status}"
            ));
        }
        response
            .json::<TokenResponse>()
            .await
            .map(|body| body.access_token)
            .map_err(|err| format!("failed to decode Google OAuth token response: {err}"))
    }

    async fn fetch_sheet_values(
        &self,
        config: &GoogleSheetsPollConfig,
        access_token: &str,
    ) -> Result<Vec<GoogleFormRow>, String> {
        let range = google_sheet_range(config);
        let encoded_range = urlencoding::encode(&range);
        let url = format!(
            "{}/{}/values/{}?majorDimension=ROWS",
            SHEETS_API_BASE,
            urlencoding::encode(&config.sheet_id),
            encoded_range
        );
        let response = self
            .http
            .get(url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|err| format!("Google Sheets values request failed: {err}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "Google Sheets values request failed with status {status}"
            ));
        }
        let body = response
            .json::<ValuesResponse>()
            .await
            .map_err(|err| format!("failed to decode Google Sheets values response: {err}"))?;
        Ok(values_to_rows(config.start_row, body.values))
    }
}

impl Default for GoogleSheetsReadonlyClient {
    fn default() -> Self {
        Self::new()
    }
}

pub fn google_sheets_read_plan(config: &GoogleSheetsPollConfig) -> GoogleSheetsReadPlan {
    GoogleSheetsReadPlan {
        range: google_sheet_range(config),
        token_uri_status: "<SET>",
        credentials_status: if config.credentials_file.as_os_str().is_empty() {
            "<MISSING>"
        } else {
            "<SET>"
        },
        sheet_id_status: if config.sheet_id.trim().is_empty() {
            "<MISSING>"
        } else {
            "<SET>"
        },
    }
}

pub fn google_sheet_range(config: &GoogleSheetsPollConfig) -> String {
    let sheet_name = config.sheet_name.trim();
    let range = format!(
        "A{}:{}",
        config.start_row.max(1),
        config.end_column.trim().trim_start_matches('$')
    );
    if sheet_name.is_empty() {
        range
    } else {
        format!("'{}'!{}", sheet_name.replace('\'', "''"), range)
    }
}

pub fn values_to_rows(start_row: i64, values: Vec<Vec<serde_json::Value>>) -> Vec<GoogleFormRow> {
    values
        .into_iter()
        .enumerate()
        .map(|(index, row)| GoogleFormRow {
            sheet_row: start_row + index as i64,
            values: row
                .into_iter()
                .map(|value| match value {
                    serde_json::Value::Null => String::new(),
                    serde_json::Value::String(value) => value,
                    other => other.to_string(),
                })
                .collect(),
        })
        .collect()
}

fn read_service_account_key(path: &Path) -> Result<ServiceAccountKey, String> {
    let text = fs::read_to_string(path).map_err(|err| {
        format!(
            "failed to read Google credentials file {}: {err}",
            path.display()
        )
    })?;
    serde_json::from_str(&text).map_err(|err| {
        format!(
            "failed to parse Google credentials JSON {}: {err}",
            path.display()
        )
    })
}
