use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use crate::domain::model::OllamaModel;

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Serialize)]
struct OllamaPullRequest {
    name: String,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaPullStreamResponse {
    status: Option<String>,
    completed: Option<i64>,
    total: Option<i64>,
    error: Option<String>,
}

pub(crate) struct PullProgress {
    pub(crate) status: String,
    pub(crate) completed: Option<i64>,
    pub(crate) total: Option<i64>,
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

pub(crate) fn pull_model_stream<F>(
    model: String,
    cancellation: Arc<AtomicBool>,
    mut on_progress: F,
) -> Result<(), String>
where
    F: FnMut(PullProgress) -> Result<(), String>,
{
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(None)
        .build()
        .map_err(|error| error.to_string())?;
    let request = OllamaPullRequest {
        name: model,
        stream: true,
    };
    let response = client
        .post("http://127.0.0.1:11434/api/pull")
        .json(&request)
        .send()
        .map_err(|_| "Ollama is not running. Open Ollama and try again.".to_string())?;
    let status = response.status();

    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        let message = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(|error| error.as_str())
                    .map(ToString::to_string)
            })
            .unwrap_or_else(|| format!("Ollama request failed with status {status}"));
        return Err(message);
    }

    let mut reader = BufReader::new(response);
    let mut line = String::new();

    loop {
        if cancellation.load(Ordering::SeqCst) {
            return Err("Job cancelled".to_string());
        }

        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if bytes_read == 0 {
            break;
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let chunk = serde_json::from_str::<OllamaPullStreamResponse>(line)
            .map_err(|error| error.to_string())?;
        if let Some(error) = chunk.error {
            return Err(error);
        }

        let status = chunk
            .status
            .unwrap_or_else(|| "Downloading model".to_string());
        let is_success = status == "success";
        on_progress(PullProgress {
            status,
            completed: chunk.completed,
            total: chunk.total,
        })?;

        if is_success {
            break;
        }
    }

    Ok(())
}
