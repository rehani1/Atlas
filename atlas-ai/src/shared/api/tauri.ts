import { invoke } from '@tauri-apps/api/core'

export type OllamaModel = {
  name: string
  size: number
}

export type GenerationRun = {
  id: string
  conversation_id: string
  message_id: number | null
  model_name: string
  started_at: number
  first_token_at: number | null
  completed_at: number | null
  status: 'running' | 'completed' | 'cancelled' | 'failed'
  total_duration_ms: number | null
  load_duration_ms: number | null
  prompt_eval_count: number | null
  prompt_eval_duration_ms: number | null
  eval_count: number | null
  eval_duration_ms: number | null
  tokens_per_second: number | null
  error_message: string | null
}

export type ChatMessage = {
  id: number
  chat_id: string
  role: 'user' | 'assistant' | 'system'
  content: string
  created_at: number
  generation_run: GenerationRun | null
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

export type ChatExportFormat = 'markdown' | 'json' | 'plain_text'

export type ChatExport = {
  file_name: string
  mime_type: string
  content: string
}

const commands = {
  getOllamaStatus: 'get_ollama_status',
  downloadOllamaModel: 'download_ollama_model',
  deleteOllamaModel: 'delete_ollama_model',
  exportChat: 'export_chat',
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

export function exportChat(chatId: string, format: ChatExportFormat) {
  return invoke<ChatExport>(commands.exportChat, { chatId, format })
}
