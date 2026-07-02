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

export type SearchSnippetPart = {
  text: string
  is_match: boolean
}

export type ChatSearchResult = {
  chat_id: string
  chat_title: string
  message_id: number | null
  role: 'user' | 'assistant' | 'system' | null
  created_at: number
  updated_at: number
  message_count: number
  source: 'title' | 'message'
  score: number
  snippet: SearchSnippetPart[]
}

export type ConversationSummary = {
  id: string
  conversation_id: string
  summary: string
  source_message_start_id: number | null
  source_message_end_id: number | null
  model_name: string
  version: number
  enabled_for_prompt: boolean
  created_at: number
  updated_at: number
}

export type ModelBenchmarkStatus =
  | 'queued'
  | 'running'
  | 'completed'
  | 'cancelled'
  | 'failed'

export type ModelBenchmark = {
  id: string
  job_id: string
  model_name: string
  prompt_type: string
  prompt_label: string
  prompt_text_hash: string
  started_at: number | null
  completed_at: number | null
  status: ModelBenchmarkStatus
  total_duration_ms: number | null
  first_token_ms: number | null
  prompt_eval_count: number | null
  prompt_eval_duration_ms: number | null
  eval_count: number | null
  eval_duration_ms: number | null
  tokens_per_second: number | null
  error_message: string | null
  created_at: number
}

export type ModelUsage = {
  model_name: string
  last_used_at: number | null
  generation_count: number
}

const commands = {
  getOllamaStatus: 'get_ollama_status',
  downloadOllamaModel: 'download_ollama_model',
  deleteOllamaModel: 'delete_ollama_model',
  exportChat: 'export_chat',
  listJobs: 'list_jobs',
  cancelJob: 'cancel_job',
  getDatabaseDiagnostics: 'get_database_diagnostics',
  searchConversations: 'search_conversations',
  getConversationSummary: 'get_conversation_summary',
  saveConversationSummary: 'save_conversation_summary',
  setConversationSummaryEnabled: 'set_conversation_summary_enabled',
  deleteConversationSummary: 'delete_conversation_summary',
  generateConversationSummary: 'generate_conversation_summary',
  listModelBenchmarks: 'list_model_benchmarks',
  listModelUsage: 'list_model_usage',
  startModelBenchmark: 'start_model_benchmark',
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

export function searchConversations(query: string, limit = 30) {
  return invoke<ChatSearchResult[]>(commands.searchConversations, {
    query,
    limit,
  })
}

export function getConversationSummary(chatId: string) {
  return invoke<ConversationSummary | null>(commands.getConversationSummary, {
    chatId,
  })
}

export function saveConversationSummary(
  chatId: string,
  summary: string,
  enabledForPrompt: boolean,
) {
  return invoke<ConversationSummary>(commands.saveConversationSummary, {
    chatId,
    summary,
    enabledForPrompt,
  })
}

export function setConversationSummaryEnabled(
  chatId: string,
  enabledForPrompt: boolean,
) {
  return invoke<ConversationSummary>(commands.setConversationSummaryEnabled, {
    chatId,
    enabledForPrompt,
  })
}

export function deleteConversationSummary(chatId: string) {
  return invoke<boolean>(commands.deleteConversationSummary, { chatId })
}

export function generateConversationSummary(chatId: string, model: string) {
  return invoke<ConversationSummary>(commands.generateConversationSummary, {
    chatId,
    model,
  })
}

export function listModelBenchmarks(limit = 100) {
  return invoke<ModelBenchmark[]>(commands.listModelBenchmarks, { limit })
}

export function listModelUsage() {
  return invoke<ModelUsage[]>(commands.listModelUsage)
}

export function startModelBenchmark(model: string) {
  return invoke<Job>(commands.startModelBenchmark, { model })
}
