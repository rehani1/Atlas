use serde::Deserialize;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};

use crate::domain::model::OllamaModel;

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

pub(crate) struct OllamaResponse {
    pub(crate) status_code: u16,
    pub(crate) body: String,
}

pub(crate) fn request(
    method: &str,
    path: &str,
    body: Option<String>,
) -> Result<OllamaResponse, String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 11434));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))
        .map_err(|_| "Ollama is not running. Open Ollama and try again.".to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(1200)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .map_err(|error| error.to_string())?;

    let body = body.unwrap_or_default();
    let request = format!(
    "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:11434\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
    body.len(),
    body
  );

    stream
        .write_all(request.as_bytes())
        .map_err(|error| error.to_string())?;

    let mut raw_response = String::new();
    stream
        .read_to_string(&mut raw_response)
        .map_err(|error| error.to_string())?;

    let (headers, body) = raw_response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "Ollama returned an invalid HTTP response".to_string())?;
    let status_code = headers
        .lines()
        .next()
        .and_then(|status| status.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| "Ollama returned an invalid HTTP status".to_string())?;

    Ok(OllamaResponse {
        status_code,
        body: body.to_string(),
    })
}

pub(crate) fn error(response: &OllamaResponse) -> String {
    if response.body.trim().is_empty() {
        return format!("Ollama request failed with status {}", response.status_code);
    }

    serde_json::from_str::<serde_json::Value>(&response.body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.as_str())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| response.body.clone())
}

pub(crate) fn read_models() -> Result<Vec<OllamaModel>, String> {
    let response = request("GET", "/api/tags", None)?;

    if !(200..300).contains(&response.status_code) {
        return Err(error(&response));
    }

    let tags = serde_json::from_str::<OllamaTagsResponse>(&response.body)
        .map_err(|error| error.to_string())?;

    Ok(tags.models)
}
