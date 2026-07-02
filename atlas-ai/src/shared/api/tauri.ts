import { invoke } from '@tauri-apps/api/core'

export type OllamaModel = {
  name: string
  size: number
}

export type OllamaStatusKind =
  | 'unavailable'
  | 'running_with_models'
  | 'running_without_models'
  | 'selected_model_missing'

export type OllamaStatus = {
  status: OllamaStatusKind
  models: OllamaModel[]
  selected_model: string | null
  error: string | null
}

const commands = {
  getOllamaStatus: 'get_ollama_status',
  downloadOllamaModel: 'download_ollama_model',
  deleteOllamaModel: 'delete_ollama_model',
} as const

export function getOllamaStatus(selectedModel?: string) {
  return invoke<OllamaStatus>(commands.getOllamaStatus, {
    selectedModel: selectedModel?.trim() || null,
  })
}

export function downloadOllamaModel(model: string) {
  return invoke<OllamaModel[]>(commands.downloadOllamaModel, { model })
}

export function deleteOllamaModel(model: string) {
  return invoke<OllamaModel[]>(commands.deleteOllamaModel, { model })
}
