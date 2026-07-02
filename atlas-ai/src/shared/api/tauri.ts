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

export type JobType =
  | 'chat_generation'
  | 'model_pull'
  | 'model_delete'
  | 'export_conversation'
  | 'document_import'
  | 'embedding_index'
  | 'model_benchmark'
  | 'conversation_summary'

export type JobStatus =
  | 'queued'
  | 'running'
  | 'cancelling'
  | 'cancelled'
  | 'succeeded'
  | 'failed'

export type Job = {
  id: string
  job_type: JobType
  status: JobStatus
  progress_current: number | null
  progress_total: number | null
  label: string
  payload_json: string | null
  result_json: string | null
  error_message: string | null
  created_at: number
  started_at: number | null
  completed_at: number | null
  cancelled_at: number | null
}

export type JobEvent = {
  job_id: string
  job_type: JobType
  job: Job
}

export type DatabaseTableCount = {
  table_name: string
  row_count: number
}

export type DatabaseDiagnostics = {
  path: string
  database_size_bytes: number
  wal_size_bytes: number
  shm_size_bytes: number
  journal_mode: string
  user_version: number
  page_count: number
  page_size: number
  freelist_count: number
  integrity_check: string
  table_counts: DatabaseTableCount[]
}

const commands = {
  getOllamaStatus: 'get_ollama_status',
  downloadOllamaModel: 'download_ollama_model',
  deleteOllamaModel: 'delete_ollama_model',
  exportChat: 'export_chat',
  listJobs: 'list_jobs',
  cancelJob: 'cancel_job',
  getDatabaseDiagnostics: 'get_database_diagnostics',
} as const

export function getOllamaStatus(selectedModel?: string) {
  return invoke<OllamaStatus>(commands.getOllamaStatus, {
    selectedModel: selectedModel?.trim() || null,
  })
}

export function downloadOllamaModel(model: string) {
  return invoke<Job>(commands.downloadOllamaModel, { model })
}

export function deleteOllamaModel(model: string) {
  return invoke<OllamaModel[]>(commands.deleteOllamaModel, { model })
}

export function exportChat(chatId: string, format: ChatExportFormat) {
  return invoke<ChatExport>(commands.exportChat, { chatId, format })
}

export function listJobs(limit = 10) {
  return invoke<Job[]>(commands.listJobs, { limit })
}

export function cancelJob(jobId: string) {
  return invoke<Job>(commands.cancelJob, { jobId })
}

export function getDatabaseDiagnostics() {
  return invoke<DatabaseDiagnostics>(commands.getDatabaseDiagnostics)
}
