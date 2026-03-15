//! Figma REST API HTTP client.
//!
//! Wraps `reqwest` to call the Figma file, variables, and images endpoints.
//! Authentication uses a Personal Access Token passed via the `X-Figma-Token` header.

use std::collections::HashMap;

use reqwest::Client;

use super::types::{FigmaFileResponse, FigmaVariablesResponse};
use crate::error::ImportError;

/// HTTP client for the Figma REST API.
pub struct FigmaClient {
    token: String,
    client: Client,
    base_url: String,
}

impl FigmaClient {
    /// Create a new client with the given Personal Access Token.
    pub fn new(token: String) -> Self {
        Self {
            token,
            client: Client::new(),
            base_url: "https://api.figma.com".into(),
        }
    }

    /// `GET /v1/files/:file_key` -- fetch the full Figma file tree.
    pub async fn get_file(&self, file_key: &str) -> Result<FigmaFileResponse, ImportError> {
        let url = format!("{}/v1/files/{}", self.base_url, file_key);
        let resp = self
            .client
            .get(&url)
            .header("X-Figma-Token", &self.token)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(ImportError::Api {
                status: resp.status().as_u16() as u32,
                message: resp.text().await.unwrap_or_default(),
            });
        }

        Ok(resp.json().await?)
    }

    /// `GET /v1/files/:file_key/variables/local` -- fetch design variables.
    pub async fn get_variables(
        &self,
        file_key: &str,
    ) -> Result<FigmaVariablesResponse, ImportError> {
        let url = format!("{}/v1/files/{}/variables/local", self.base_url, file_key);
        let resp = self
            .client
            .get(&url)
            .header("X-Figma-Token", &self.token)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(ImportError::Api {
                status: resp.status().as_u16() as u32,
                message: resp.text().await.unwrap_or_default(),
            });
        }

        Ok(resp.json().await?)
    }

    /// Fetch image fill assets.
    ///
    /// Uses `GET /v1/files/:key/images` to resolve `imageRef` values to URLs,
    /// then downloads each referenced image. Individual image download failures
    /// are silently skipped (non-fatal).
    pub async fn get_images(
        &self,
        file_key: &str,
        image_refs: &[String],
    ) -> Result<HashMap<String, Vec<u8>>, ImportError> {
        if image_refs.is_empty() {
            return Ok(HashMap::new());
        }

        let url = format!("{}/v1/files/{}/images", self.base_url, file_key);
        let resp = self
            .client
            .get(&url)
            .header("X-Figma-Token", &self.token)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(ImportError::Api {
                status: resp.status().as_u16() as u32,
                message: resp.text().await.unwrap_or_default(),
            });
        }

        let body: serde_json::Value = resp.json().await?;

        let mut result = HashMap::new();
        if let Some(images) = body
            .get("meta")
            .and_then(|m| m.get("images"))
            .and_then(|i| i.as_object())
        {
            for (ref_id, url_val) in images {
                if !image_refs.contains(ref_id) {
                    continue;
                }
                if let Some(image_url) = url_val.as_str() {
                    match self.client.get(image_url).send().await {
                        Ok(r) if r.status().is_success() => {
                            if let Ok(bytes) = r.bytes().await {
                                result.insert(ref_id.clone(), bytes.to_vec());
                            }
                        }
                        _ => {} // individual image failure is non-fatal
                    }
                }
            }
        }

        Ok(result)
    }
}
