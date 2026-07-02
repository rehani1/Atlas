use crate::{
    domain::model::{build_ollama_status, OllamaModel, OllamaStatus},
    infra::ollama,
};

pub(crate) fn list_models() -> Result<Vec<OllamaModel>, String> {
    ollama::read_models()
}

pub(crate) fn get_status(selected_model: Option<String>) -> OllamaStatus {
    match list_models() {
        Ok(models) => build_ollama_status(models, selected_model, None),
        Err(error) => build_ollama_status(Vec::new(), selected_model, Some(error)),
    }
}
