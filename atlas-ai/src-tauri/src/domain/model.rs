use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub(crate) struct OllamaModel {
    pub(crate) name: String,
    pub(crate) size: i64,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OllamaStatusKind {
    Unavailable,
    RunningWithModels,
    RunningWithoutModels,
    SelectedModelMissing,
}

#[derive(Serialize)]
pub(crate) struct OllamaStatus {
    pub(crate) status: OllamaStatusKind,
    pub(crate) models: Vec<OllamaModel>,
    pub(crate) selected_model: Option<String>,
    pub(crate) error: Option<String>,
}

pub(crate) fn validate_ollama_model_name(model: &str) -> Result<String, String> {
    let model = model.trim();

    if model.is_empty() {
        return Err("Model name cannot be empty".to_string());
    }

    if model
        .chars()
        .any(|character| character.is_whitespace() || character == '"' || character == '\\')
    {
        return Err("Model name contains invalid characters".to_string());
    }

    Ok(model.to_string())
}

fn normalize_selected_model(selected_model: Option<String>) -> Option<String> {
    selected_model
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty())
}

pub(crate) fn build_ollama_status(
    models: Vec<OllamaModel>,
    selected_model: Option<String>,
    error: Option<String>,
) -> OllamaStatus {
    let selected_model = normalize_selected_model(selected_model);
    let status = if error.is_some() {
        OllamaStatusKind::Unavailable
    } else if models.is_empty() {
        OllamaStatusKind::RunningWithoutModels
    } else if selected_model.as_ref().is_some_and(|selected_model| {
        !models
            .iter()
            .any(|model| model.name == selected_model.as_str())
    }) {
        OllamaStatusKind::SelectedModelMissing
    } else {
        OllamaStatusKind::RunningWithModels
    };

    OllamaStatus {
        status,
        models,
        selected_model,
        error,
    }
}
