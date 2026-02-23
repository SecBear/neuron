//! OpenAI Embeddings API implementation.

use std::future::Future;

use neuron_types::{
    EmbeddingError, EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, EmbeddingUsage,
};

use crate::client::OpenAi;

/// Default embedding model used when none is specified on the request.
const DEFAULT_EMBEDDING_MODEL: &str = "text-embedding-3-small";

impl OpenAi {
    /// Build the embeddings endpoint URL.
    pub(crate) fn embeddings_url(&self) -> String {
        format!("{}/v1/embeddings", self.base_url)
    }
}

/// Map an HTTP status code from the embeddings endpoint to an [`EmbeddingError`].
fn map_embedding_http_status(status: reqwest::StatusCode, body: &str) -> EmbeddingError {
    match status.as_u16() {
        401 | 403 => EmbeddingError::Authentication(body.to_string()),
        429 => EmbeddingError::RateLimit {
            retry_after: parse_retry_after(body),
        },
        400 | 404 => EmbeddingError::InvalidRequest(body.to_string()),
        _ => EmbeddingError::Other(body.to_string().into()),
    }
}

/// Attempt to parse a retry delay from an OpenAI error body.
fn parse_retry_after(body: &str) -> Option<std::time::Duration> {
    let lower = body.to_lowercase();
    if let Some(idx) = lower.find("retry after ") {
        let after = &lower[idx + 12..];
        let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(secs) = num_str.parse::<u64>() {
            return Some(std::time::Duration::from_secs(secs));
        }
    }
    None
}

/// Map a [`reqwest::Error`] to an [`EmbeddingError`].
fn map_reqwest_error(err: reqwest::Error) -> EmbeddingError {
    EmbeddingError::Network(Box::new(err))
}

impl EmbeddingProvider for OpenAi {
    /// Generate embeddings via the OpenAI Embeddings API.
    ///
    /// Sends a POST request to `{base_url}/v1/embeddings` with the model,
    /// input texts, and optional dimensions. Returns one embedding vector
    /// per input string.
    fn embed(
        &self,
        request: EmbeddingRequest,
    ) -> impl Future<Output = Result<EmbeddingResponse, EmbeddingError>> + Send {
        let url = self.embeddings_url();
        let api_key = self.api_key.clone();
        let organization = self.organization.clone();
        let http_client = self.client.clone();

        async move {
            let model = if request.model.is_empty() {
                DEFAULT_EMBEDDING_MODEL.to_string()
            } else {
                request.model
            };

            let mut body = serde_json::json!({
                "model": model,
                "input": request.input,
                "encoding_format": "float",
            });

            if let Some(dims) = request.dimensions {
                body["dimensions"] = serde_json::json!(dims);
            }

            // Merge extra fields into the body
            if let serde_json::Value::Object(body_map) = &mut body {
                for (key, value) in request.extra {
                    body_map.insert(key, value);
                }
            }

            tracing::debug!(url = %url, model = %body["model"], "sending embedding request");

            let mut req = http_client
                .post(&url)
                .header("authorization", format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .json(&body);

            if let Some(org) = &organization {
                req = req.header("openai-organization", org);
            }

            let response = req.send().await.map_err(map_reqwest_error)?;

            let status = response.status();
            let response_text = response.text().await.map_err(map_reqwest_error)?;

            if !status.is_success() {
                return Err(map_embedding_http_status(status, &response_text));
            }

            let json: serde_json::Value = serde_json::from_str(&response_text).map_err(|e| {
                EmbeddingError::InvalidRequest(format!("invalid JSON response: {e}"))
            })?;

            parse_embedding_response(&json, &model)
        }
    }
}

/// Parse an OpenAI embeddings API response into an [`EmbeddingResponse`].
fn parse_embedding_response(
    json: &serde_json::Value,
    default_model: &str,
) -> Result<EmbeddingResponse, EmbeddingError> {
    let data = json["data"]
        .as_array()
        .ok_or_else(|| EmbeddingError::InvalidRequest("missing 'data' array".to_string()))?;

    let mut embeddings = Vec::with_capacity(data.len());
    for item in data {
        let embedding = item["embedding"]
            .as_array()
            .ok_or_else(|| {
                EmbeddingError::InvalidRequest("missing 'embedding' array in data item".to_string())
            })?
            .iter()
            .map(|v| {
                v.as_f64().map(|f| f as f32).ok_or_else(|| {
                    EmbeddingError::InvalidRequest("non-numeric value in embedding".to_string())
                })
            })
            .collect::<Result<Vec<f32>, _>>()?;
        embeddings.push(embedding);
    }

    let model = json["model"].as_str().unwrap_or(default_model).to_string();

    let usage = &json["usage"];
    let prompt_tokens = usage["prompt_tokens"].as_u64().unwrap_or(0) as usize;
    let total_tokens = usage["total_tokens"].as_u64().unwrap_or(0) as usize;

    Ok(EmbeddingResponse {
        embeddings,
        model,
        usage: EmbeddingUsage {
            prompt_tokens,
            total_tokens,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_embedding_model_is_set() {
        assert_eq!(DEFAULT_EMBEDDING_MODEL, "text-embedding-3-small");
    }

    #[test]
    fn embeddings_url_includes_path() {
        let client = OpenAi::new("test-key").base_url("http://localhost:9999");
        assert_eq!(
            client.embeddings_url(),
            "http://localhost:9999/v1/embeddings"
        );
    }

    #[test]
    fn map_401_to_authentication() {
        let err = map_embedding_http_status(reqwest::StatusCode::UNAUTHORIZED, "bad key");
        assert!(matches!(err, EmbeddingError::Authentication(_)));
    }

    #[test]
    fn map_403_to_authentication() {
        let err = map_embedding_http_status(reqwest::StatusCode::FORBIDDEN, "forbidden");
        assert!(matches!(err, EmbeddingError::Authentication(_)));
    }

    #[test]
    fn map_429_to_rate_limit() {
        let err = map_embedding_http_status(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            "Rate limit exceeded",
        );
        assert!(matches!(err, EmbeddingError::RateLimit { .. }));
    }

    #[test]
    fn map_400_to_invalid_request() {
        let err = map_embedding_http_status(reqwest::StatusCode::BAD_REQUEST, "bad request");
        assert!(matches!(err, EmbeddingError::InvalidRequest(_)));
    }

    #[test]
    fn map_404_to_invalid_request() {
        let err = map_embedding_http_status(reqwest::StatusCode::NOT_FOUND, "not found");
        assert!(matches!(err, EmbeddingError::InvalidRequest(_)));
    }

    #[test]
    fn map_500_to_other() {
        let err =
            map_embedding_http_status(reqwest::StatusCode::INTERNAL_SERVER_ERROR, "server error");
        assert!(matches!(err, EmbeddingError::Other(_)));
    }

    #[test]
    fn parse_valid_response() {
        let json = serde_json::json!({
            "data": [
                { "embedding": [0.1, 0.2, 0.3], "index": 0 },
                { "embedding": [0.4, 0.5, 0.6], "index": 1 }
            ],
            "model": "text-embedding-3-small",
            "usage": { "prompt_tokens": 10, "total_tokens": 10 }
        });

        let resp = parse_embedding_response(&json, "fallback").unwrap();
        assert_eq!(resp.embeddings.len(), 2);
        assert_eq!(resp.embeddings[0], vec![0.1, 0.2, 0.3]);
        assert_eq!(resp.embeddings[1], vec![0.4, 0.5, 0.6]);
        assert_eq!(resp.model, "text-embedding-3-small");
        assert_eq!(resp.usage.prompt_tokens, 10);
        assert_eq!(resp.usage.total_tokens, 10);
    }

    #[test]
    fn parse_response_uses_fallback_model() {
        let json = serde_json::json!({
            "data": [{ "embedding": [1.0], "index": 0 }],
            "usage": { "prompt_tokens": 1, "total_tokens": 1 }
        });

        let resp = parse_embedding_response(&json, "fallback-model").unwrap();
        assert_eq!(resp.model, "fallback-model");
    }

    #[test]
    fn parse_response_missing_data_is_error() {
        let json = serde_json::json!({ "model": "test" });
        let err = parse_embedding_response(&json, "test").unwrap_err();
        assert!(matches!(err, EmbeddingError::InvalidRequest(_)));
    }

    #[test]
    fn parse_response_missing_embedding_array_is_error() {
        let json = serde_json::json!({
            "data": [{ "index": 0 }],
            "model": "test",
            "usage": { "prompt_tokens": 1, "total_tokens": 1 }
        });
        let err = parse_embedding_response(&json, "test").unwrap_err();
        assert!(matches!(err, EmbeddingError::InvalidRequest(_)));
    }

    #[test]
    fn parse_response_non_numeric_embedding_value_is_error() {
        let json = serde_json::json!({
            "data": [{ "embedding": [0.1, "not_a_number", 0.3], "index": 0 }],
            "model": "test",
            "usage": { "prompt_tokens": 1, "total_tokens": 1 }
        });
        let err = parse_embedding_response(&json, "test").unwrap_err();
        assert!(matches!(err, EmbeddingError::InvalidRequest(_)));
    }

    #[test]
    fn parse_response_missing_usage_defaults_to_zero() {
        let json = serde_json::json!({
            "data": [{ "embedding": [1.0, 2.0], "index": 0 }],
            "model": "test"
        });
        let resp = parse_embedding_response(&json, "test").unwrap();
        assert_eq!(resp.usage.prompt_tokens, 0);
        assert_eq!(resp.usage.total_tokens, 0);
    }

    #[test]
    fn parse_retry_after_extracts_seconds() {
        let result = parse_retry_after("Rate limit exceeded. Please retry after 45 seconds.");
        assert_eq!(result, Some(std::time::Duration::from_secs(45)));
    }

    #[test]
    fn parse_retry_after_returns_none_when_not_present() {
        let result = parse_retry_after("Generic error with no retry info");
        assert_eq!(result, None);
    }

    #[test]
    fn parse_retry_after_case_insensitive() {
        let result = parse_retry_after("RETRY AFTER 30 seconds");
        assert_eq!(result, Some(std::time::Duration::from_secs(30)));
    }

    #[test]
    fn map_429_with_retry_after_in_message() {
        let err = map_embedding_http_status(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            "Please retry after 60 seconds",
        );
        if let EmbeddingError::RateLimit { retry_after } = err {
            assert_eq!(retry_after, Some(std::time::Duration::from_secs(60)));
        } else {
            panic!("expected RateLimit, got: {err:?}");
        }
    }
}
