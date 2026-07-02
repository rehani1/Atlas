import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import {
  type FormEvent,
  type KeyboardEvent,
  useEffect,
  useRef,
  useState,
} from 'react'
import {
  cancelJob,
  archiveMemory,
  createMemory,
  deleteOllamaModel,
  deleteConversationSummary,
  deleteMemory,
  downloadOllamaModel,
  exportChat,
  generateConversationSummary,
  getConversationSummary,
  getDatabaseDiagnostics,
  getKnowledgePromptSetting,
  getMemoryPromptSetting,
  getOllamaStatus,
  indexKnowledgePath,
  listKnowledgeDocuments,
  listKnowledgeWorkspaces,
  listModelBenchmarks,
  listModelUsage,
  listJobs,
  listMemories,
  removeKnowledgeWorkspace,
  restoreMemory,
  saveConversationSummary,
  searchConversations,
  searchKnowledgeDocuments,
  setConversationSummaryEnabled,
  setKnowledgePromptEnabled,
  setMemoryPromptEnabled,
  startModelBenchmark,
  updateMemory,
  type ChatExport,
  type ChatExportFormat,
  type ChatMessage,
  type ChatSearchResult,
  type ConversationSummary,
  type DatabaseDiagnostics,
  type DocumentSearchResult,
  type GenerationContextItem,
  type GenerationDocumentSourceUse,
  type GenerationRun,
  type Job,
  type JobEvent,
  type KnowledgeDocument,
  type KnowledgePromptSetting,
  type KnowledgeWorkspace,
  type Memory,
  type MemoryPromptSetting,
  type MemoryScopeType,
  type ModelBenchmark,
  type ModelUsage,
  type OllamaModel,
  type OllamaStatus,
} from './shared/api/tauri'

type IconProps = {
  className?: string
}

type ChatSummary = {
  id: string
  title: string
  created_at: number
  updated_at: number
  message_count: number
}

type UiError = {
  message: string
  details?: string
}

type SourcePreview = {
  title: string
  path: string
  lineRange: string
  content: string
}

type CommandId =
  | 'chat.new'
  | 'chat.search'
  | 'chat.delete_active'
  | 'chat.summary.open'
  | 'memory.inspector.open'
  | 'model.manager.open'
  | 'model.refresh'
  | 'model.switch.open'
  | `model.switch:${string}`
  | `model.download:${string}`
  | `chat.export.${ChatExportFormat}`
  | 'settings.open'
  | 'diagnostics.open'
  | 'model_lab.open'
  | 'knowledge.workspace.open'
  | 'knowledge.index_folder'

type AppCommand = {
  id: CommandId
  title: string
  category: 'Chat' | 'Model' | 'Memory' | 'App' | 'Knowledge'
  description?: string
  disabledReason?: string
  keywords?: string[]
  run: () => void | Promise<void>
}

type CommandRegistryContext = {
  activeChatId: string | null
  deletingChatId: string | null
  exportAction: ChatExportFormat | null
  hasActiveConversationSummaryJob: boolean
  hasActiveKnowledgeIndexJob: boolean
  hasActiveModelBenchmarkJob: boolean
  hasActiveModelPullJob: boolean
  isDesktop: boolean
  isOllamaStatusLoading: boolean
  modelAction: string | null
  ollamaModels: OllamaModel[]
  ollamaStatus: OllamaStatus | null
  selectedModel: string
  onDeleteActiveChat: () => Promise<void>
  onDownloadModel: (model: string) => Promise<void>
  onExportChat: (format: ChatExportFormat) => Promise<void>
  onNewChat: () => Promise<void>
  onOpenChatSearch: () => void
  onOpenConversationSummary: () => void
  onOpenDiagnostics: () => void
  onOpenKnowledgeWorkspace: () => void
  onOpenMemoryInspector: () => void
  onOpenModelLab: () => void
  onOpenModelManager: () => void
  onRefreshModels: () => Promise<void>
  onSelectModel: (model: string) => void
}

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

const isTauriRuntime = () => window.__TAURI_INTERNALS__ !== undefined
const recommendedModels = [
  { name: 'llama3.2:1b', note: 'Fastest' },
  { name: 'llama3.2:3b', note: 'Best default' },
]
const chatExportFormats: { format: ChatExportFormat; label: string }[] = [
  { format: 'markdown', label: 'Markdown' },
  { format: 'json', label: 'JSON' },
  { format: 'plain_text', label: 'Plain text' },
]
const browserOllamaStatus: OllamaStatus = {
  status: 'unavailable',
  models: [],
  selected_model: null,
  error: 'Model management is available in the Tauri desktop app.',
}

function titleFromMessage(content: string) {
  const title = content.trim()
  return title.length > 48 ? `${title.slice(0, 45)}...` : title
}

function formatModelSize(size: number) {
  if (size <= 0) {
    return 'Unknown size'
  }

  const units = ['B', 'KB', 'MB', 'GB']
  let value = size
  let unitIndex = 0

  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex += 1
  }

  return `${value.toFixed(unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`
}

function formatBytes(size: number) {
  if (size <= 0) {
    return '0 B'
  }

  const units = ['B', 'KB', 'MB', 'GB']
  let value = size
  let unitIndex = 0

  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex += 1
  }

  return `${value.toFixed(unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`
}

function formatDurationMs(durationMs: number | null | undefined) {
  if (durationMs === null || durationMs === undefined) {
    return null
  }

  if (durationMs < 1000) {
    return `${durationMs} ms`
  }

  const seconds = durationMs / 1000
  return `${seconds.toFixed(seconds < 10 ? 2 : 1)} s`
}

function formatCount(count: number | null | undefined) {
  return count === null || count === undefined ? null : count.toLocaleString()
}

function formatTokensPerSecond(value: number | null | undefined) {
  return value === null || value === undefined ? null : `${value.toFixed(1)} tok/s`
}

function formatSearchTimestamp(timestamp: number) {
  return new Intl.DateTimeFormat(undefined, {
    month: 'short',
    day: 'numeric',
  }).format(new Date(timestamp))
}

function searchResultSourceLabel(result: ChatSearchResult) {
  if (result.source === 'title') {
    return 'Title match'
  }

  return result.role ? `${result.role} message` : 'Message match'
}

function formatTimestamp(timestamp: number | null | undefined) {
  if (timestamp === null || timestamp === undefined) {
    return 'Never'
  }

  return new Intl.DateTimeFormat(undefined, {
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  }).format(new Date(timestamp))
}

function formatBenchmarkStatus(status: ModelBenchmark['status']) {
  switch (status) {
    case 'queued':
      return 'Queued'
    case 'running':
      return 'Running'
    case 'completed':
      return 'Completed'
    case 'cancelled':
      return 'Cancelled'
    case 'failed':
      return 'Failed'
  }
}

function getEvalSpeed(count: number | null, durationMs: number | null) {
  if (count === null || durationMs === null || count <= 0 || durationMs <= 0) {
    return null
  }

  return count / (durationMs / 1000)
}

function formatSpeed(value: number | null | undefined) {
  return value === null || value === undefined ? 'n/a' : `${value.toFixed(1)} tok/s`
}

function getTimeToFirstToken(run: GenerationRun) {
  if (run.first_token_at === null) {
    return null
  }

  return Math.max(0, run.first_token_at - run.started_at)
}

function generationStatusLabel(status: GenerationRun['status']) {
  switch (status) {
    case 'completed':
      return 'Completed'
    case 'cancelled':
      return 'Cancelled'
    case 'failed':
      return 'Failed'
    case 'running':
      return 'Running'
  }
}

function contextItemTypeLabel(itemType: GenerationContextItem['item_type']) {
  switch (itemType) {
    case 'system_prompt':
      return 'System'
    case 'summary':
      return 'Summary'
    case 'memory':
      return 'Memory'
    case 'prior_message':
      return 'Prior message'
    case 'document_chunk':
      return 'Document chunk'
    case 'user_message':
      return 'User message'
    case 'model_options':
      return 'Model options'
    case 'truncation_notice':
      return 'Truncation'
  }
}

function parseContextMetadata(item: GenerationContextItem) {
  if (!item.metadata_json) {
    return null
  }

  try {
    const value = JSON.parse(item.metadata_json) as Record<string, unknown>
    return value
  } catch {
    return null
  }
}

function contextMetadataLine(item: GenerationContextItem) {
  const metadata = parseContextMetadata(item)
  if (!metadata) {
    return null
  }

  if (typeof metadata.hash === 'string') {
    return `Hash ${metadata.hash}`
  }

  if (typeof metadata.model === 'string') {
    return `Model ${metadata.model}`
  }

  if (typeof metadata.source_id === 'string') {
    return `Source ${metadata.source_id}`
  }

  if (typeof metadata.omitted_prior_message_count === 'string') {
    return `${metadata.omitted_prior_message_count} omitted`
  }

  if (typeof metadata.preview === 'string') {
    return metadata.preview
  }

  return null
}

function getContextPromptEstimate(run: GenerationRun) {
  const total = run.context_items.reduce(
    (sum, item) => sum + item.token_count_estimate,
    0,
  )

  return total > 0 ? total : null
}

function MessageDiagnostics({
  run,
  onOpenSource,
}: {
  run: GenerationRun | null
  onOpenSource: (source: GenerationDocumentSourceUse) => void
}) {
  if (!run) {
    return null
  }

  const metrics = [
    ['Model', run.model_name],
    ['Status', generationStatusLabel(run.status)],
    ['Total', formatDurationMs(run.total_duration_ms)],
    ['First token', formatDurationMs(getTimeToFirstToken(run))],
    ['Prompt', formatCount(run.prompt_eval_count)],
    ['Context est.', formatCount(getContextPromptEstimate(run))],
    ['Completion', formatCount(run.eval_count)],
    ['Speed', formatTokensPerSecond(run.tokens_per_second)],
    ['Load', formatDurationMs(run.load_duration_ms)],
  ].filter((metric): metric is [string, string] => metric[1] !== null)

  return (
    <details className="mt-3 border-t border-zinc-800/80 pt-2 text-xs text-zinc-500">
      <summary className="cursor-pointer text-zinc-400">
        {run.status === 'completed'
          ? 'Details'
          : generationStatusLabel(run.status)}
      </summary>
      <dl className="mt-2 grid grid-cols-2 gap-x-4 gap-y-1">
        {metrics.map(([label, value]) => (
          <div className="min-w-0" key={label}>
            <dt className="text-zinc-600">{label}</dt>
            <dd className="truncate text-zinc-300">{value}</dd>
          </div>
        ))}
      </dl>
      {run.error_message ? (
        <p className="mt-2 break-words text-zinc-400">{run.error_message}</p>
      ) : null}
      {run.context_items.length > 0 ? (
        <div className="mt-3 border-t border-zinc-800/80 pt-2">
          <p className="text-zinc-400">
            Prompt context: {run.context_items.length} items
          </p>
          <div className="mt-2 grid gap-1.5">
            {run.context_items.map((item) => {
              const metadataLine = contextMetadataLine(item)

              return (
                <div
                  className={`rounded-lg px-2 py-1.5 ${
                    item.item_type === 'truncation_notice'
                      ? 'bg-amber-950/20 text-amber-100'
                      : 'bg-zinc-950 text-zinc-400'
                  }`}
                  key={item.id}
                >
                  <div className="flex items-start justify-between gap-2">
                    <p className="min-w-0 break-words text-zinc-300">
                      {item.label}
                    </p>
                    <span className="flex-none rounded bg-zinc-900 px-1.5 py-0.5 text-[10px] text-zinc-500">
                      {contextItemTypeLabel(item.item_type)}
                    </span>
                  </div>
                  <p className="mt-1 text-[11px] text-zinc-600">
                    {item.token_count_estimate.toLocaleString()} est. tokens
                    {item.item_id ? ` - ${item.item_id}` : ''}
                  </p>
                  {metadataLine ? (
                    <p className="mt-1 break-words text-[11px] text-zinc-500">
                      {truncateText(metadataLine, 220)}
                    </p>
                  ) : null}
                </div>
              )
            })}
          </div>
        </div>
      ) : null}
      {run.memory_uses.length > 0 ? (
        <div className="mt-3 border-t border-zinc-800/80 pt-2">
          <p className="text-zinc-400">
            Memory used: {run.memory_uses.length}
          </p>
          <div className="mt-2 grid gap-1.5">
            {run.memory_uses.map((memoryUse) => (
              <div
                className="rounded-lg bg-zinc-950 px-2 py-1.5 text-zinc-400"
                key={memoryUse.id}
              >
                <p className="break-words">
                  {truncateText(memoryUse.content, 180)}
                </p>
                <p className="mt-1 text-[11px] text-zinc-600">
                  {memoryUse.scope_type}
                  {memoryUse.source_message_id !== null
                    ? ` - source message ${memoryUse.source_message_id}`
                    : ''}
                </p>
              </div>
            ))}
          </div>
        </div>
      ) : null}
      {run.document_sources.length > 0 ? (
        <div className="mt-3 border-t border-zinc-800/80 pt-2">
          <p className="text-zinc-400">
            Sources used: {run.document_sources.length}
          </p>
          <div className="mt-2 grid gap-1.5">
            {run.document_sources.map((source) => (
              <button
                className="rounded-lg bg-zinc-950 px-2 py-1.5 text-left text-zinc-400 transition-colors hover:bg-zinc-800 hover:text-zinc-100"
                key={source.id}
                type="button"
                onClick={() => onOpenSource(source)}
              >
                <span className="block truncate text-zinc-300">
                  [{source.source_id}] {source.file_name}
                </span>
                <span className="mt-1 block text-[11px] text-zinc-600">
                  Lines {source.start_line}-{source.end_line}
                </span>
                <span className="mt-1 block break-words">
                  {truncateText(source.content, 180)}
                </span>
              </button>
            ))}
          </div>
        </div>
      ) : null}
    </details>
  )
}

function buildOllamaStatusFromModels(
  models: OllamaModel[],
  selectedModel?: string,
): OllamaStatus {
  const selected = selectedModel?.trim() || null
  const selectedModelMissing =
    selected !== null && !models.some((model) => model.name === selected)

  return {
    status:
      models.length === 0
        ? 'running_without_models'
        : selectedModelMissing
          ? 'selected_model_missing'
          : 'running_with_models',
    models,
    selected_model: selected,
    error: null,
  }
}

function chooseSelectedModel(
  status: OllamaStatus,
  currentModel: string,
  preferredModel?: string,
) {
  const preferred = preferredModel?.trim()
  if (preferred && status.models.some((model) => model.name === preferred)) {
    return preferred
  }

  const current = currentModel.trim()
  if (current && status.models.some((model) => model.name === current)) {
    return current
  }

  if (
    status.status === 'unavailable' ||
    status.status === 'selected_model_missing'
  ) {
    return status.selected_model ?? current
  }

  return status.models[0]?.name ?? ''
}

function getReadinessNotice(
  status: OllamaStatus | null,
  selectedModel: string,
  isDesktop: boolean,
) {
  if (!isDesktop) {
    return {
      title: 'Browser preview',
      body: 'Open the Tauri desktop app to chat with local models.',
      details: undefined,
    }
  }

  if (!status) {
    return null
  }

  if (status.status === 'unavailable') {
    return {
      title: 'Ollama is offline',
      body: 'Start Ollama locally, then retry.',
      details: status.error ?? undefined,
    }
  }

  if (status.status === 'running_without_models') {
    return {
      title: 'No local models',
      body: 'Download a model to start chatting.',
      details: undefined,
    }
  }

  if (status.status === 'selected_model_missing') {
    const missingModel = selectedModel || status.selected_model || 'Selected model'
    return {
      title: 'Selected model missing',
      body: `${missingModel} is not installed.`,
      details: undefined,
    }
  }

  return null
}

function getModelPanelEmptyText(status: OllamaStatus | null) {
  if (status?.status === 'unavailable') {
    return 'Ollama is offline.'
  }

  if (status?.status === 'running_without_models') {
    return 'No local models installed.'
  }

  return 'No local models found.'
}

function getChatBlockReason(status: OllamaStatus | null, selectedModel: string) {
  if (status?.status === 'unavailable') {
    return 'Ollama is offline. Start Ollama, then retry.'
  }

  if (status?.status === 'running_without_models') {
    return 'Download a local model before chatting.'
  }

  if (status?.status === 'selected_model_missing') {
    return 'Selected model is not installed. Pick another model.'
  }

  if (!selectedModel) {
    return 'Select a local model before chatting.'
  }

  return null
}

function getRefreshModelsDisabledReason(context: CommandRegistryContext) {
  if (!context.isDesktop) {
    return 'Model refresh requires the Tauri desktop app.'
  }

  if (context.modelAction !== null) {
    return 'Finish the current model action first.'
  }

  if (context.isOllamaStatusLoading) {
    return 'Model status is already refreshing.'
  }

  return undefined
}

function getDownloadModelDisabledReason(
  context: CommandRegistryContext,
  modelName: string,
) {
  if (context.ollamaModels.some((model) => model.name === modelName)) {
    return 'Already installed.'
  }

  if (!context.isDesktop) {
    return 'Model downloads require the Tauri desktop app.'
  }

  if (context.modelAction !== null) {
    return 'Finish the current model action first.'
  }

  if (context.hasActiveModelPullJob) {
    return 'Wait for the current model download to finish.'
  }

  if (context.isOllamaStatusLoading) {
    return 'Wait for model status to finish refreshing.'
  }

  if (context.ollamaStatus?.status === 'unavailable') {
    return 'Ollama is offline.'
  }

  return undefined
}

function getExportChatDisabledReason(context: CommandRegistryContext) {
  if (!context.activeChatId) {
    return 'Open a chat before exporting.'
  }

  if (!context.isDesktop) {
    return 'Chat export requires the Tauri desktop app.'
  }

  if (context.exportAction !== null) {
    return 'Wait for the current export to finish.'
  }

  return undefined
}

function getConversationSummaryDisabledReason(context: CommandRegistryContext) {
  if (!context.activeChatId) {
    return 'Open a chat before summarizing.'
  }

  if (!context.isDesktop) {
    return 'Conversation summaries require the Tauri desktop app.'
  }

  if (context.hasActiveConversationSummaryJob) {
    return 'Wait for the current summary job to finish.'
  }

  return undefined
}

function saveChatExport(exportedChat: ChatExport) {
  const blob = new Blob([exportedChat.content], {
    type: exportedChat.mime_type,
  })
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  link.href = url
  link.download = exportedChat.file_name
  link.rel = 'noopener'
  document.body.append(link)
  link.click()
  link.remove()
  window.setTimeout(() => URL.revokeObjectURL(url), 0)
}

function upsertJob(jobs: Job[], job: Job) {
  return [job, ...jobs.filter((currentJob) => currentJob.id !== job.id)]
    .sort((first, second) => second.created_at - first.created_at)
    .slice(0, 10)
}

function isActiveJob(job: Job) {
  return (
    job.status === 'queued' ||
    job.status === 'running' ||
    job.status === 'cancelling'
  )
}

function jobStatusLabel(status: Job['status']) {
  switch (status) {
    case 'queued':
      return 'Queued'
    case 'running':
      return 'Running'
    case 'cancelling':
      return 'Cancelling'
    case 'cancelled':
      return 'Cancelled'
    case 'succeeded':
      return 'Succeeded'
    case 'failed':
      return 'Failed'
  }
}

function getJobProgressPercent(job: Job) {
  if (
    job.progress_current === null ||
    job.progress_total === null ||
    job.progress_total <= 0
  ) {
    return null
  }

  return Math.min(100, Math.max(0, (job.progress_current / job.progress_total) * 100))
}

function formatJobProgress(job: Job) {
  if (job.progress_current === null || job.progress_total === null) {
    return jobStatusLabel(job.status)
  }

  if (job.job_type === 'model_benchmark') {
    return `${job.progress_current} / ${job.progress_total} prompts`
  }

  if (job.job_type === 'conversation_summary') {
    return `${job.progress_current} / ${job.progress_total} steps`
  }

  if (job.job_type === 'document_import') {
    return `${job.progress_current} / ${job.progress_total} files`
  }

  const currentMb = job.progress_current / 1024 / 1024
  const totalMb = job.progress_total / 1024 / 1024
  return `${currentMb.toFixed(1)} / ${totalMb.toFixed(1)} MB`
}

function getJobPayload(job: Job) {
  if (!job.payload_json) {
    return null
  }

  try {
    const payload = JSON.parse(job.payload_json) as Record<string, unknown>
    return payload
  } catch {
    return null
  }
}

function getJobChatId(job: Job) {
  const payload = getJobPayload(job)
  return typeof payload?.chat_id === 'string' ? payload.chat_id : null
}

function formatSummaryRange(summary: ConversationSummary | null) {
  if (
    summary?.source_message_start_id === null ||
    summary?.source_message_start_id === undefined ||
    summary.source_message_end_id === null ||
    summary.source_message_end_id === undefined
  ) {
    return 'No source range'
  }

  if (summary.source_message_start_id === summary.source_message_end_id) {
    return `Message ${summary.source_message_start_id}`
  }

  return `Messages ${summary.source_message_start_id}-${summary.source_message_end_id}`
}

function formatMemoryScope(memory: Pick<Memory, 'scope_type' | 'scope_id'>) {
  switch (memory.scope_type) {
    case 'global':
      return 'Global'
    case 'conversation':
      return memory.scope_id ? 'Conversation' : 'Conversation'
    case 'project':
      return memory.scope_id ? `Project ${memory.scope_id}` : 'Project'
  }
}

function formatMemorySource(memory: Memory) {
  if (memory.source_message_id !== null) {
    return `Message ${memory.source_message_id}`
  }

  if (memory.source_conversation_id !== null) {
    return 'Conversation source'
  }

  return 'Manual'
}

function truncateText(value: string, maxLength = 120) {
  return value.length > maxLength ? `${value.slice(0, maxLength - 3)}...` : value
}

function buildCommandRegistry(context: CommandRegistryContext): AppCommand[] {
  const commands: AppCommand[] = [
    {
      id: 'chat.new',
      title: 'New chat',
      category: 'Chat',
      description: 'Start a blank conversation.',
      keywords: ['conversation', 'clear'],
      run: context.onNewChat,
    },
    {
      id: 'chat.search',
      title: 'Search chats',
      category: 'Chat',
      description: 'Search saved conversations.',
      keywords: ['find', 'history'],
      run: context.onOpenChatSearch,
    },
    {
      id: 'chat.delete_active',
      title: 'Delete current chat',
      category: 'Chat',
      description: 'Remove the open conversation.',
      disabledReason: context.activeChatId
        ? context.deletingChatId === context.activeChatId
          ? 'Current chat is already being deleted.'
          : undefined
        : 'Open a chat before deleting.',
      keywords: ['remove conversation'],
      run: context.onDeleteActiveChat,
    },
    {
      id: 'model.manager.open',
      title: 'Open model manager',
      category: 'Model',
      description: 'Manage local Ollama models.',
      keywords: ['ollama models'],
      run: context.onOpenModelManager,
    },
    {
      id: 'model.refresh',
      title: 'Refresh models',
      category: 'Model',
      description: 'Reload Ollama readiness and installed models.',
      disabledReason: getRefreshModelsDisabledReason(context),
      keywords: ['ollama reload retry'],
      run: context.onRefreshModels,
    },
  ]

  if (context.ollamaModels.length === 0) {
    commands.push({
      id: 'model.switch.open',
      title: 'Switch model',
      category: 'Model',
      description: 'Choose another installed local model.',
      disabledReason: 'No local models are available.',
      keywords: ['select change ollama'],
      run: context.onOpenModelManager,
    })
  } else {
    context.ollamaModels.forEach((model) => {
      commands.push({
        id: `model.switch:${model.name}`,
        title: `Switch to ${model.name}`,
        category: 'Model',
        description: formatModelSize(model.size),
        disabledReason:
          context.selectedModel === model.name ? 'Already selected.' : undefined,
        keywords: ['switch select model ollama'],
        run: () => context.onSelectModel(model.name),
      })
    })
  }

  recommendedModels.forEach((model) => {
    commands.push({
      id: `model.download:${model.name}`,
      title: `Download ${model.name}`,
      category: 'Model',
      description: model.note,
      disabledReason: getDownloadModelDisabledReason(context, model.name),
      keywords: ['install pull ollama'],
      run: () => context.onDownloadModel(model.name),
    })
  })

  chatExportFormats.forEach((format) => {
    commands.push({
      id: `chat.export.${format.format}`,
      title: `Export current chat as ${format.label}`,
      category: 'Chat',
      description: 'Save the open conversation locally.',
      disabledReason: getExportChatDisabledReason(context),
      keywords: ['download save transcript'],
      run: () => context.onExportChat(format.format),
    })
  })

  commands.push({
    id: 'chat.summary.open',
    title: 'Open chat summary',
    category: 'Chat',
    description: 'Inspect or update the current conversation summary.',
    disabledReason: getConversationSummaryDisabledReason(context),
    keywords: ['summarize context continuity'],
    run: context.onOpenConversationSummary,
  })

  commands.push({
    id: 'knowledge.workspace.open',
    title: 'Open Knowledge Workspace',
    category: 'Knowledge',
    description: context.hasActiveKnowledgeIndexJob
      ? 'Review the active indexing job.'
      : 'Index and search local text/code files.',
    disabledReason: context.isDesktop
      ? undefined
      : 'Knowledge workspace requires the Tauri desktop app.',
    keywords: ['rag documents files citations index folder'],
    run: context.onOpenKnowledgeWorkspace,
  })

  commands.push(
    {
      id: 'memory.inspector.open',
      title: 'Open Memory Inspector',
      category: 'Memory',
      description: 'Review stored memories.',
      disabledReason: context.isDesktop
        ? undefined
        : 'Memory Inspector requires the Tauri desktop app.',
      keywords: ['remember preferences context'],
      run: context.onOpenMemoryInspector,
    },
    {
      id: 'settings.open',
      title: 'Open settings',
      category: 'App',
      description: 'Configure Atlas.',
      disabledReason: 'Settings have not been added yet.',
      keywords: ['preferences'],
      run: () => undefined,
    },
    {
      id: 'diagnostics.open',
      title: 'Open diagnostics',
      category: 'App',
      description: 'Review local database health.',
      disabledReason: context.isDesktop
        ? undefined
        : 'Diagnostics require the Tauri desktop app.',
      keywords: ['health status'],
      run: context.onOpenDiagnostics,
    },
    {
      id: 'model_lab.open',
      title: 'Open Model Lab',
      category: 'Model',
      description: 'Benchmark installed local models.',
      disabledReason: context.isDesktop
        ? undefined
        : 'Model Lab requires the Tauri desktop app.',
      keywords: ['benchmark evaluate'],
      run: context.onOpenModelLab,
    },
  )

  return commands
}

function normalizeCommandText(value: string) {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, ' ').trim()
}

function scoreCommand(command: AppCommand, query: string) {
  const normalizedQuery = normalizeCommandText(query)
  if (!normalizedQuery) {
    return 1000
  }

  const target = normalizeCommandText(
    [
      command.title,
      command.description,
      command.category,
      command.id,
      ...(command.keywords ?? []),
    ].join(' '),
  )

  if (target.includes(normalizedQuery)) {
    return 900 - target.indexOf(normalizedQuery)
  }

  const tokens = normalizedQuery.split(/\s+/).filter(Boolean)
  if (tokens.length > 1 && tokens.every((token) => target.includes(token))) {
    return 700 - tokens.reduce((sum, token) => sum + target.indexOf(token), 0)
  }

  const compactQuery = normalizedQuery.replace(/\s+/g, '')
  const compactTarget = target.replace(/\s+/g, '')
  let queryIndex = 0
  let score = 0

  for (
    let targetIndex = 0;
    targetIndex < compactTarget.length && queryIndex < compactQuery.length;
    targetIndex += 1
  ) {
    if (compactTarget[targetIndex] === compactQuery[queryIndex]) {
      score += targetIndex === queryIndex ? 4 : 1
      queryIndex += 1
    }
  }

  return queryIndex === compactQuery.length ? score : null
}

function filterCommands(commands: AppCommand[], query: string) {
  return commands
    .map((command, index) => ({
      command,
      index,
      score: scoreCommand(command, query),
    }))
    .filter(
      (
        entry,
      ): entry is { command: AppCommand; index: number; score: number } =>
        entry.score !== null,
    )
    .sort((first, second) => second.score - first.score || first.index - second.index)
    .map((entry) => entry.command)
}

function ShipWheelLogo({ className = 'h-7 w-7' }: IconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 32 32"
      fill="none"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <circle cx="16" cy="16" r="8.5" strokeWidth="2" />
      <circle cx="16" cy="16" r="2.4" strokeWidth="2" />
      <path strokeWidth="2" d="M16 2.5v7" />
      <path strokeWidth="2" d="M16 22.5v7" />
      <path strokeWidth="2" d="M2.5 16h7" />
      <path strokeWidth="2" d="M22.5 16h7" />
      <path strokeWidth="2" d="m6.5 6.5 5 5" />
      <path strokeWidth="2" d="m20.5 20.5 5 5" />
      <path strokeWidth="2" d="m25.5 6.5-5 5" />
      <path strokeWidth="2" d="m11.5 20.5-5 5" />
      <circle cx="16" cy="2.5" r="1.7" fill="currentColor" stroke="none" />
      <circle cx="16" cy="29.5" r="1.7" fill="currentColor" stroke="none" />
      <circle cx="2.5" cy="16" r="1.7" fill="currentColor" stroke="none" />
      <circle cx="29.5" cy="16" r="1.7" fill="currentColor" stroke="none" />
    </svg>
  )
}

function SidebarToggleIcon({ className = 'h-6 w-6' }: IconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <rect x="3" y="4" width="18" height="16" rx="4" />
      <path d="M9 4v16" />
    </svg>
  )
}

function NewChatIcon({ className = 'h-5.5 w-5.5' }: IconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
      <path d="M18.4 2.6a2.1 2.1 0 0 1 3 3L12 15l-4 1 1-4Z" />
    </svg>
  )
}

function SearchIcon({ className = 'h-5.5 w-5.5' }: IconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <circle cx="11" cy="11" r="7" />
      <path d="m20 20-4.6-4.6" />
    </svg>
  )
}

function XIcon({ className = 'h-4 w-4' }: IconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M18 6 6 18" />
      <path d="m6 6 12 12" />
    </svg>
  )
}

function TrashIcon({ className = 'h-5 w-5' }: IconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M3 6h18" />
      <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
      <path d="M19 6 18 20a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" />
      <path d="M10 11v6" />
      <path d="M14 11v6" />
    </svg>
  )
}

function DownloadIcon({ className = 'h-5 w-5' }: IconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M12 3v11" />
      <path d="m7 9 5 5 5-5" />
      <path d="M5 20h14" />
    </svg>
  )
}

function SummaryIcon({ className = 'h-5 w-5' }: IconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M6 3h9l3 3v15H6Z" />
      <path d="M14 3v4h4" />
      <path d="M9 11h6" />
      <path d="M9 15h6" />
      <path d="M9 19h4" />
    </svg>
  )
}

function MemoryIcon({ className = 'h-5 w-5' }: IconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M12 3a6 6 0 0 0-4 10.47V18h8v-4.53A6 6 0 0 0 12 3Z" />
      <path d="M9 21h6" />
      <path d="M10 18h4" />
      <path d="M9.5 9.5h.01" />
      <path d="M14.5 9.5h.01" />
      <path d="M10 13h4" />
    </svg>
  )
}

function KnowledgeIcon({ className = 'h-5 w-5' }: IconProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M4 5a2 2 0 0 1 2-2h12v16H6a2 2 0 0 0-2 2Z" />
      <path d="M8 7h6" />
      <path d="M8 11h8" />
      <path d="M8 15h5" />
    </svg>
  )
}

function App() {
  const [isSidebarOpen, setIsSidebarOpen] = useState(true)
  const [chats, setChats] = useState<ChatSummary[]>([])
  const [activeChatId, setActiveChatId] = useState<string | null>(null)
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [activeSummary, setActiveSummary] = useState<ConversationSummary | null>(
    null,
  )
  const [summaryDraft, setSummaryDraft] = useState('')
  const [isSummaryPanelOpen, setIsSummaryPanelOpen] = useState(false)
  const [isSummaryLoading, setIsSummaryLoading] = useState(false)
  const [summaryAction, setSummaryAction] = useState<
    'generate' | 'save' | 'delete' | 'toggle' | null
  >(null)
  const [summaryError, setSummaryError] = useState<string | null>(null)
  const [memories, setMemories] = useState<Memory[]>([])
  const [memoryPromptSetting, setMemoryPromptSetting] =
    useState<MemoryPromptSetting | null>(null)
  const [isMemoryInspectorOpen, setIsMemoryInspectorOpen] = useState(false)
  const [isMemoryLoading, setIsMemoryLoading] = useState(false)
  const [memoryError, setMemoryError] = useState<string | null>(null)
  const [memoryAction, setMemoryAction] = useState<string | null>(null)
  const [memoryDraft, setMemoryDraft] = useState('')
  const [memoryScope, setMemoryScope] = useState<MemoryScopeType>('global')
  const [memoryPinned, setMemoryPinned] = useState(false)
  const [editingMemoryId, setEditingMemoryId] = useState<string | null>(null)
  const [memorySource, setMemorySource] = useState<{
    conversationId: string
    messageId: number
    label: string
  } | null>(null)
  const [knowledgeWorkspaces, setKnowledgeWorkspaces] = useState<
    KnowledgeWorkspace[]
  >([])
  const [knowledgeDocuments, setKnowledgeDocuments] = useState<
    KnowledgeDocument[]
  >([])
  const [knowledgePromptSetting, setKnowledgePromptSetting] =
    useState<KnowledgePromptSetting | null>(null)
  const [isKnowledgeWorkspaceOpen, setIsKnowledgeWorkspaceOpen] =
    useState(false)
  const [isKnowledgeLoading, setIsKnowledgeLoading] = useState(false)
  const [knowledgeError, setKnowledgeError] = useState<string | null>(null)
  const [knowledgeAction, setKnowledgeAction] = useState<string | null>(null)
  const [knowledgePath, setKnowledgePath] = useState('')
  const [knowledgeSearchQuery, setKnowledgeSearchQuery] = useState('')
  const [knowledgeSearchResults, setKnowledgeSearchResults] = useState<
    DocumentSearchResult[]
  >([])
  const [sourcePreview, setSourcePreview] = useState<SourcePreview | null>(null)
  const [draft, setDraft] = useState('')
  const [historyError, setHistoryError] = useState<string | null>(null)
  const [ollamaStatus, setOllamaStatus] = useState<OllamaStatus | null>(() =>
    isTauriRuntime() ? null : browserOllamaStatus,
  )
  const [isOllamaStatusLoading, setIsOllamaStatusLoading] = useState(() =>
    isTauriRuntime(),
  )
  const [ollamaModels, setOllamaModels] = useState<OllamaModel[]>([])
  const [selectedModel, setSelectedModel] = useState('')
  const [modelError, setModelError] = useState<UiError | null>(null)
  const [modelAction, setModelAction] = useState<string | null>(null)
  const [modelBenchmarks, setModelBenchmarks] = useState<ModelBenchmark[]>([])
  const [modelUsage, setModelUsage] = useState<ModelUsage[]>([])
  const [isModelLabOpen, setIsModelLabOpen] = useState(false)
  const [isModelLabLoading, setIsModelLabLoading] = useState(false)
  const [modelLabAction, setModelLabAction] = useState<string | null>(null)
  const [modelLabError, setModelLabError] = useState<string | null>(null)
  const [jobs, setJobs] = useState<Job[]>([])
  const [isModelPanelOpen, setIsModelPanelOpen] = useState(false)
  const [isResponding, setIsResponding] = useState(false)
  const [respondingChatId, setRespondingChatId] = useState<string | null>(null)
  const [deletingChatId, setDeletingChatId] = useState<string | null>(null)
  const [exportAction, setExportAction] = useState<ChatExportFormat | null>(null)
  const [isExportMenuOpen, setIsExportMenuOpen] = useState(false)
  const [isChatSearchOpen, setIsChatSearchOpen] = useState(false)
  const [chatSearchQuery, setChatSearchQuery] = useState('')
  const [chatSearchResults, setChatSearchResults] = useState<ChatSearchResult[]>([])
  const [isChatSearchLoading, setIsChatSearchLoading] = useState(false)
  const [chatSearchError, setChatSearchError] = useState<string | null>(null)
  const [pendingSearchJump, setPendingSearchJump] = useState<{
    chatId: string
    messageId: number
  } | null>(null)
  const [highlightedMessageId, setHighlightedMessageId] = useState<number | null>(
    null,
  )
  const [isCommandPaletteOpen, setIsCommandPaletteOpen] = useState(false)
  const [commandQuery, setCommandQuery] = useState('')
  const [activeCommandIndex, setActiveCommandIndex] = useState(0)
  const [isDiagnosticsOpen, setIsDiagnosticsOpen] = useState(false)
  const [databaseDiagnostics, setDatabaseDiagnostics] =
    useState<DatabaseDiagnostics | null>(null)
  const [isDatabaseDiagnosticsLoading, setIsDatabaseDiagnosticsLoading] =
    useState(false)
  const [databaseDiagnosticsError, setDatabaseDiagnosticsError] = useState<
    string | null
  >(null)
  const activeChatIdRef = useRef<string | null>(null)
  const commandInputRef = useRef<HTMLInputElement | null>(null)
  const messageRefs = useRef<Map<number, HTMLElement>>(new Map())

  useEffect(() => {
    activeChatIdRef.current = activeChatId
  }, [activeChatId])

  useEffect(() => {
    if (!isTauriRuntime()) {
      return
    }

    let ignore = false

    invoke<ChatSummary[]>('list_chats')
      .then((loadedChats) => {
        if (!ignore) {
          setChats(loadedChats)
          setHistoryError(null)
        }
      })
      .catch((error: unknown) => {
        if (!ignore) {
          setHistoryError(String(error))
        }
      })

    return () => {
      ignore = true
    }
  }, [])

  useEffect(() => {
    if (!isTauriRuntime()) {
      return
    }

    let ignore = false

    getOllamaStatus()
      .then((status) => {
        if (!ignore) {
          setOllamaStatus(status)
          setOllamaModels(status.models)
          setSelectedModel((currentModel) => {
            return chooseSelectedModel(status, currentModel)
          })
          setModelError(null)
        }
      })
      .catch((error: unknown) => {
        if (!ignore) {
          const details = String(error)
          setOllamaStatus({
            ...browserOllamaStatus,
            error: details,
          })
          setModelError({
            message: 'Could not check Ollama status.',
            details,
          })
        }
      })
      .finally(() => {
        if (!ignore) {
          setIsOllamaStatusLoading(false)
        }
      })

    return () => {
      ignore = true
    }
  }, [])

  useEffect(() => {
    if (!isTauriRuntime()) {
      return
    }

    let ignore = false
    let unlisten: (() => void) | undefined

    listJobs(10)
      .then((loadedJobs) => {
        if (!ignore) {
          setJobs(loadedJobs)
        }
      })
      .catch((error: unknown) => {
        if (!ignore) {
          setHistoryError(String(error))
        }
      })

    listen<JobEvent>('job_updated', (event) => {
      if (!ignore) {
        setJobs((currentJobs) => upsertJob(currentJobs, event.payload.job))
        if (
          event.payload.job.job_type === 'model_benchmark' &&
          !isActiveJob(event.payload.job)
        ) {
          void refreshModelLabData(false)
        }
        if (
          event.payload.job.job_type === 'document_import' &&
          !isActiveJob(event.payload.job)
        ) {
          void Promise.all([listKnowledgeWorkspaces(), listKnowledgeDocuments(100)])
            .then(([workspaces, documents]) => {
              if (!ignore) {
                setKnowledgeWorkspaces(workspaces)
                setKnowledgeDocuments(documents)
              }
            })
            .catch((error: unknown) => {
              if (!ignore) {
                setKnowledgeError(String(error))
              }
            })
        }
        const eventChatId = getJobChatId(event.payload.job)
        if (
          event.payload.job.job_type === 'conversation_summary' &&
          !isActiveJob(event.payload.job) &&
          eventChatId !== null &&
          eventChatId === activeChatIdRef.current
        ) {
          void refreshSummaryForChat(eventChatId, false)
        }
      }
    })
      .then((listener) => {
        if (ignore) {
          listener()
          return
        }

        unlisten = listener
      })
      .catch((error: unknown) => {
        if (!ignore) {
          setHistoryError(String(error))
        }
      })

    return () => {
      ignore = true
      unlisten?.()
    }
  }, [])

  useEffect(() => {
    if (!isTauriRuntime()) {
      return
    }

    let ignore = false

    Promise.all([listKnowledgeWorkspaces(), listKnowledgeDocuments(100)])
      .then(([workspaces, documents]) => {
        if (!ignore) {
          setKnowledgeWorkspaces(workspaces)
          setKnowledgeDocuments(documents)
        }
      })
      .catch((error: unknown) => {
        if (!ignore) {
          setKnowledgeError(String(error))
        }
      })

    return () => {
      ignore = true
    }
  }, [])

  useEffect(() => {
    if (!activeChatId) {
      return
    }

    if (!isTauriRuntime()) {
      return
    }

    let ignore = false

    invoke<ChatMessage[]>('get_messages', { chatId: activeChatId })
      .then((loadedMessages) => {
        if (!ignore) {
          setMessages(loadedMessages)
          setHistoryError(null)
        }
      })
      .catch((error: unknown) => {
        if (!ignore) {
          setHistoryError(String(error))
        }
      })

    return () => {
      ignore = true
    }
  }, [activeChatId])

  useEffect(() => {
    if (!activeChatId) {
      return
    }

    if (!isTauriRuntime()) {
      return
    }

    let ignore = false
    const chatId = activeChatId

    getConversationSummary(chatId)
      .then((summary) => {
        if (!ignore && activeChatIdRef.current === chatId) {
          setActiveSummary(summary)
          setSummaryDraft(summary?.summary ?? '')
          setSummaryError(null)
        }
      })
      .catch((error: unknown) => {
        if (!ignore && activeChatIdRef.current === chatId) {
          setSummaryError(String(error))
        }
      })

    return () => {
      ignore = true
    }
  }, [activeChatId])

  useEffect(() => {
    if (!activeChatId || !isTauriRuntime()) {
      return
    }

    let ignore = false
    const chatId = activeChatId

    Promise.all([getMemoryPromptSetting(chatId), listMemories(false)])
      .then(([setting, loadedMemories]) => {
        if (!ignore && activeChatIdRef.current === chatId) {
          setMemoryPromptSetting(setting)
          setMemories((currentMemories) => {
            const archivedMemories = currentMemories.filter(
              (memory) => memory.archived_at !== null,
            )
            return [
              ...loadedMemories,
              ...archivedMemories.filter(
                (memory) =>
                  !loadedMemories.some(
                    (loadedMemory) => loadedMemory.id === memory.id,
                  ),
              ),
            ]
          })
          setMemoryError(null)
        }
      })
      .catch((error: unknown) => {
        if (!ignore && activeChatIdRef.current === chatId) {
          setMemoryError(String(error))
        }
      })

    return () => {
      ignore = true
    }
  }, [activeChatId])

  useEffect(() => {
    if (!activeChatId || !isTauriRuntime()) {
      return
    }

    let ignore = false
    const chatId = activeChatId

    getKnowledgePromptSetting(chatId)
      .then((setting) => {
        if (!ignore && activeChatIdRef.current === chatId) {
          setKnowledgePromptSetting(setting)
          setKnowledgeError(null)
        }
      })
      .catch((error: unknown) => {
        if (!ignore && activeChatIdRef.current === chatId) {
          setKnowledgeError(String(error))
        }
      })

    return () => {
      ignore = true
    }
  }, [activeChatId])

  useEffect(() => {
    if (!pendingSearchJump || pendingSearchJump.chatId !== activeChatId) {
      return
    }

    const animationFrame = window.requestAnimationFrame(() => {
      const messageElement = messageRefs.current.get(pendingSearchJump.messageId)

      if (!messageElement) {
        return
      }

      messageElement.scrollIntoView({ block: 'center', behavior: 'smooth' })
      setHighlightedMessageId(pendingSearchJump.messageId)
      setPendingSearchJump(null)
    })

    return () => {
      window.cancelAnimationFrame(animationFrame)
    }
  }, [activeChatId, messages, pendingSearchJump])

  useEffect(() => {
    if (highlightedMessageId === null) {
      return
    }

    const timeout = window.setTimeout(() => {
      setHighlightedMessageId(null)
    }, 2400)

    return () => {
      window.clearTimeout(timeout)
    }
  }, [highlightedMessageId])

  useEffect(() => {
    function handleKeyDown(event: globalThis.KeyboardEvent) {
      const isCommandShortcut =
        (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k'

      if (isCommandShortcut) {
        event.preventDefault()
        setCommandQuery('')
        setActiveCommandIndex(0)
        setIsCommandPaletteOpen((isOpen) => !isOpen)
        return
      }

      if (event.key === 'Escape' && isCommandPaletteOpen) {
        event.preventDefault()
        setCommandQuery('')
        setActiveCommandIndex(0)
        setIsCommandPaletteOpen(false)
      }
    }

    window.addEventListener('keydown', handleKeyDown)

    return () => {
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [isCommandPaletteOpen])

  useEffect(() => {
    if (!isCommandPaletteOpen) {
      return
    }

    const animationFrame = window.requestAnimationFrame(() => {
      commandInputRef.current?.focus()
    })

    return () => {
      window.cancelAnimationFrame(animationFrame)
    }
  }, [isCommandPaletteOpen])

  useEffect(() => {
    const query = chatSearchQuery.trim()

    if (!isChatSearchOpen || !query || !isTauriRuntime()) {
      return
    }

    let ignore = false
    const timeout = window.setTimeout(() => {
      setIsChatSearchLoading(true)

      searchConversations(query)
        .then((results) => {
          if (!ignore) {
            setChatSearchResults(results)
            setChatSearchError(null)
          }
        })
        .catch((error: unknown) => {
          if (!ignore) {
            setChatSearchError(String(error))
          }
        })
        .finally(() => {
          if (!ignore) {
            setIsChatSearchLoading(false)
          }
        })
    }, 180)

    return () => {
      ignore = true
      window.clearTimeout(timeout)
    }
  }, [isChatSearchOpen, chatSearchQuery])

  useEffect(() => {
    const query = knowledgeSearchQuery.trim()

    if (!isKnowledgeWorkspaceOpen || !query || !isTauriRuntime()) {
      return
    }

    let ignore = false
    const timeout = window.setTimeout(() => {
      searchKnowledgeDocuments(query, 20)
        .then((results) => {
          if (!ignore) {
            setKnowledgeSearchResults(results)
            setKnowledgeError(null)
          }
        })
        .catch((error: unknown) => {
          if (!ignore) {
            setKnowledgeError(String(error))
          }
        })
    }, 180)

    return () => {
      ignore = true
      window.clearTimeout(timeout)
    }
  }, [isKnowledgeWorkspaceOpen, knowledgeSearchQuery])

  async function refreshChats() {
    if (!isTauriRuntime()) {
      return
    }

    const loadedChats = await invoke<ChatSummary[]>('list_chats')
    setChats(loadedChats)
  }

  function upsertChat(chat: ChatSummary) {
    setChats((currentChats) => [
      chat,
      ...currentChats.filter((currentChat) => currentChat.id !== chat.id),
    ])
  }

  function applyOllamaStatus(status: OllamaStatus, preferredModel?: string) {
    setOllamaStatus(status)
    setOllamaModels(status.models)
    setSelectedModel((currentModel) => {
      return chooseSelectedModel(status, currentModel, preferredModel)
    })
  }

  async function refreshOllamaModels() {
    if (!isTauriRuntime()) {
      setOllamaStatus(browserOllamaStatus)
      setModelError({
        message: 'Model management requires the Tauri desktop app.',
      })
      return
    }

    try {
      setModelAction('refresh')
      setIsOllamaStatusLoading(true)
      applyOllamaStatus(await getOllamaStatus(selectedModel))
      setModelError(null)
    } catch (error) {
      setModelError({
        message: 'Could not refresh Ollama status.',
        details: String(error),
      })
    } finally {
      setIsOllamaStatusLoading(false)
      setModelAction(null)
    }
  }

  async function refreshSummaryForChat(chatId: string, showLoading = true) {
    try {
      if (showLoading) {
        setIsSummaryLoading(true)
      }
      const summary = await getConversationSummary(chatId)
      if (activeChatIdRef.current === chatId) {
        setActiveSummary(summary)
        setSummaryDraft(summary?.summary ?? '')
        setSummaryError(null)
      }
    } catch (error) {
      if (activeChatIdRef.current === chatId) {
        setSummaryError(String(error))
      }
    } finally {
      if (showLoading) {
        setIsSummaryLoading(false)
      }
    }
  }

  async function refreshActiveSummary(showLoading = true) {
    if (!activeChatId || !isTauriRuntime()) {
      setActiveSummary(null)
      setSummaryDraft('')
      return
    }

    await refreshSummaryForChat(activeChatId, showLoading)
  }

  function openConversationSummary() {
    if (!activeChatId) {
      setHistoryError('Open a chat before summarizing.')
      return
    }

    setIsSummaryPanelOpen(true)
    void refreshActiveSummary()
  }

  async function handleGenerateSummary() {
    if (!activeChatId || !isTauriRuntime()) {
      setSummaryError(
        activeChatId
          ? 'Conversation summaries require the Tauri desktop app.'
          : 'Open a chat before summarizing.',
      )
      return
    }

    const chatBlockReason = getChatBlockReason(ollamaStatus, selectedModel)
    if (chatBlockReason) {
      setSummaryError(chatBlockReason)
      return
    }

    const chatId = activeChatId

    try {
      setSummaryAction('generate')
      const summary = await generateConversationSummary(chatId, selectedModel)
      if (activeChatIdRef.current === chatId) {
        setActiveSummary(summary)
        setSummaryDraft(summary.summary)
      }
      setSummaryError(null)
    } catch (error) {
      const details = String(error)
      if (details !== 'Job cancelled') {
        setSummaryError(details)
      }
      await refreshActiveSummary(false)
    } finally {
      setSummaryAction(null)
    }
  }

  async function handleSaveSummary() {
    if (!activeChatId || !isTauriRuntime()) {
      setSummaryError(
        activeChatId
          ? 'Conversation summaries require the Tauri desktop app.'
          : 'Open a chat before summarizing.',
      )
      return
    }

    const chatId = activeChatId

    try {
      setSummaryAction('save')
      const summary = await saveConversationSummary(
        chatId,
        summaryDraft,
        activeSummary?.enabled_for_prompt ?? false,
      )
      if (activeChatIdRef.current === chatId) {
        setActiveSummary(summary)
        setSummaryDraft(summary.summary)
      }
      setSummaryError(null)
    } catch (error) {
      setSummaryError(String(error))
    } finally {
      setSummaryAction(null)
    }
  }

  async function handleToggleSummaryUse(enabledForPrompt: boolean) {
    if (!activeChatId || !activeSummary || !isTauriRuntime()) {
      return
    }

    const chatId = activeChatId

    try {
      setSummaryAction('toggle')
      const summary = await setConversationSummaryEnabled(chatId, enabledForPrompt)
      if (activeChatIdRef.current === chatId) {
        setActiveSummary(summary)
        setSummaryDraft(summary.summary)
      }
      setSummaryError(null)
    } catch (error) {
      setSummaryError(String(error))
    } finally {
      setSummaryAction(null)
    }
  }

  async function handleDeleteSummary() {
    if (!activeChatId || !isTauriRuntime()) {
      return
    }

    const chatId = activeChatId

    try {
      setSummaryAction('delete')
      await deleteConversationSummary(chatId)
      if (activeChatIdRef.current === chatId) {
        setActiveSummary(null)
        setSummaryDraft('')
      }
      setSummaryError(null)
    } catch (error) {
      setSummaryError(String(error))
    } finally {
      setSummaryAction(null)
    }
  }

  async function refreshMemoryData(showLoading = true) {
    if (!isTauriRuntime()) {
      setMemoryError('Memory Inspector requires the Tauri desktop app.')
      return
    }

    try {
      if (showLoading) {
        setIsMemoryLoading(true)
      }
      const [loadedMemories, setting] = await Promise.all([
        listMemories(true),
        activeChatId
          ? getMemoryPromptSetting(activeChatId)
          : Promise.resolve(null),
      ])
      setMemories(loadedMemories)
      setMemoryPromptSetting(setting)
      setMemoryError(null)
    } catch (error) {
      setMemoryError(String(error))
    } finally {
      if (showLoading) {
        setIsMemoryLoading(false)
      }
    }
  }

  function resetMemoryForm() {
    setEditingMemoryId(null)
    setMemoryDraft('')
    setMemoryScope(activeChatId ? 'conversation' : 'global')
    setMemoryPinned(false)
    setMemorySource(null)
  }

  function openMemoryInspector() {
    setIsMemoryInspectorOpen(true)
    if (!memoryDraft.trim() && !editingMemoryId) {
      setMemoryScope(activeChatId ? 'conversation' : 'global')
    }
    void refreshMemoryData()
  }

  async function handleSaveMemory() {
    if (!isTauriRuntime()) {
      setMemoryError('Memory Inspector requires the Tauri desktop app.')
      return
    }

    const scopeId = memoryScope === 'conversation' ? activeChatId : null
    if (memoryScope === 'conversation' && !scopeId) {
      setMemoryError('Open a chat before creating conversation memory.')
      return
    }

    try {
      setMemoryAction(editingMemoryId ? `save:${editingMemoryId}` : 'create')
      const savedMemory = editingMemoryId
        ? await updateMemory(editingMemoryId, memoryDraft, memoryPinned)
        : await createMemory(
            memoryScope,
            scopeId,
            memoryDraft,
            memorySource?.conversationId ?? null,
            memorySource?.messageId ?? null,
            memoryPinned,
          )
      setMemories((currentMemories) =>
        [savedMemory, ...currentMemories.filter((memory) => memory.id !== savedMemory.id)]
          .sort((first, second) => Number(second.pinned) - Number(first.pinned) || second.updated_at - first.updated_at),
      )
      resetMemoryForm()
      setMemoryError(null)
    } catch (error) {
      setMemoryError(String(error))
    } finally {
      setMemoryAction(null)
    }
  }

  function handleEditMemory(memory: Memory) {
    setEditingMemoryId(memory.id)
    setMemoryDraft(memory.content)
    setMemoryScope(memory.scope_type)
    setMemoryPinned(memory.pinned)
    setMemorySource(
      memory.source_conversation_id && memory.source_message_id
        ? {
            conversationId: memory.source_conversation_id,
            messageId: memory.source_message_id,
            label: `Message ${memory.source_message_id}`,
          }
        : null,
    )
  }

  async function handleArchiveMemory(memory: Memory) {
    if (!isTauriRuntime()) {
      return
    }

    try {
      setMemoryAction(`archive:${memory.id}`)
      const updatedMemory =
        memory.archived_at === null
          ? await archiveMemory(memory.id)
          : await restoreMemory(memory.id)
      setMemories((currentMemories) =>
        currentMemories.map((currentMemory) =>
          currentMemory.id === updatedMemory.id ? updatedMemory : currentMemory,
        ),
      )
      setMemoryError(null)
    } catch (error) {
      setMemoryError(String(error))
    } finally {
      setMemoryAction(null)
    }
  }

  async function handleDeleteMemory(memory: Memory) {
    if (!isTauriRuntime()) {
      return
    }

    try {
      setMemoryAction(`delete:${memory.id}`)
      await deleteMemory(memory.id)
      setMemories((currentMemories) =>
        currentMemories.filter((currentMemory) => currentMemory.id !== memory.id),
      )
      if (editingMemoryId === memory.id) {
        resetMemoryForm()
      }
      setMemoryError(null)
    } catch (error) {
      setMemoryError(String(error))
    } finally {
      setMemoryAction(null)
    }
  }

  async function handleSetMemoryPromptEnabled(enabledForPrompt: boolean) {
    if (!activeChatId || !isTauriRuntime()) {
      setMemoryError(
        activeChatId
          ? 'Memory Inspector requires the Tauri desktop app.'
          : 'Open a chat before enabling memory.',
      )
      return
    }

    const chatId = activeChatId

    try {
      setMemoryAction('toggle-prompt')
      const setting = await setMemoryPromptEnabled(chatId, enabledForPrompt)
      if (activeChatIdRef.current === chatId) {
        setMemoryPromptSetting(setting)
      }
      setMemoryError(null)
    } catch (error) {
      setMemoryError(String(error))
    } finally {
      setMemoryAction(null)
    }
  }

  function handleRememberMessage(message: ChatMessage) {
    if (!activeChatId || !isTauriRuntime()) {
      setHistoryError('Memory Inspector requires the Tauri desktop app.')
      return
    }

    setIsMemoryInspectorOpen(true)
    setEditingMemoryId(null)
    setMemoryScope('conversation')
    setMemoryPinned(false)
    setMemoryDraft(message.content)
    setMemorySource({
      conversationId: activeChatId,
      messageId: message.id,
      label: `${message.role} message ${message.id}`,
    })
    void refreshMemoryData(false)
  }

  function handleOpenMemorySource(memory: Memory) {
    if (!memory.source_conversation_id) {
      return
    }

    setActiveChatId(memory.source_conversation_id)
    setActiveSummary(null)
    setSummaryDraft('')
    setSummaryError(null)
    setMemoryPromptSetting(null)
    setKnowledgePromptSetting(null)
    setIsExportMenuOpen(false)
    setPendingSearchJump(
      memory.source_message_id === null
        ? null
        : {
            chatId: memory.source_conversation_id,
            messageId: memory.source_message_id,
          },
    )
    setIsMemoryInspectorOpen(false)
  }

  async function refreshKnowledgeData(showLoading = true) {
    if (!isTauriRuntime()) {
      setKnowledgeError('Knowledge workspace requires the Tauri desktop app.')
      return
    }

    try {
      if (showLoading) {
        setIsKnowledgeLoading(true)
      }
      const [workspaces, documents, setting] = await Promise.all([
        listKnowledgeWorkspaces(),
        listKnowledgeDocuments(100),
        activeChatId
          ? getKnowledgePromptSetting(activeChatId)
          : Promise.resolve(null),
      ])
      setKnowledgeWorkspaces(workspaces)
      setKnowledgeDocuments(documents)
      setKnowledgePromptSetting(setting)
      setKnowledgeError(null)
    } catch (error) {
      setKnowledgeError(String(error))
    } finally {
      if (showLoading) {
        setIsKnowledgeLoading(false)
      }
    }
  }

  function openKnowledgeWorkspace() {
    setIsKnowledgeWorkspaceOpen(true)
    void refreshKnowledgeData()
  }

  async function handleIndexKnowledgePath() {
    if (!isTauriRuntime()) {
      setKnowledgeError('Knowledge workspace requires the Tauri desktop app.')
      return
    }

    try {
      setKnowledgeAction('index')
      const job = await indexKnowledgePath(knowledgePath)
      setJobs((currentJobs) => upsertJob(currentJobs, job))
      setKnowledgePath('')
      await refreshKnowledgeData(false)
      setKnowledgeError(null)
    } catch (error) {
      const details = String(error)
      if (details !== 'Job cancelled') {
        setKnowledgeError(details)
      }
      await refreshKnowledgeData(false)
    } finally {
      setKnowledgeAction(null)
    }
  }

  async function handleRemoveKnowledgeWorkspace(workspace: KnowledgeWorkspace) {
    if (!isTauriRuntime()) {
      return
    }

    try {
      setKnowledgeAction(`remove:${workspace.id}`)
      await removeKnowledgeWorkspace(workspace.id)
      await refreshKnowledgeData(false)
      setKnowledgeError(null)
    } catch (error) {
      setKnowledgeError(String(error))
    } finally {
      setKnowledgeAction(null)
    }
  }

  async function handleSetKnowledgePromptEnabled(enabledForPrompt: boolean) {
    if (!activeChatId || !isTauriRuntime()) {
      setKnowledgeError(
        activeChatId
          ? 'Knowledge workspace requires the Tauri desktop app.'
          : 'Open a chat before enabling knowledge.',
      )
      return
    }

    const chatId = activeChatId

    try {
      setKnowledgeAction('toggle-prompt')
      const setting = await setKnowledgePromptEnabled(chatId, enabledForPrompt)
      if (activeChatIdRef.current === chatId) {
        setKnowledgePromptSetting(setting)
      }
      setKnowledgeError(null)
    } catch (error) {
      setKnowledgeError(String(error))
    } finally {
      setKnowledgeAction(null)
    }
  }

  function openGenerationSource(source: GenerationDocumentSourceUse) {
    setSourcePreview({
      title: `[${source.source_id}] ${source.file_name}`,
      path: source.path,
      lineRange: `Lines ${source.start_line}-${source.end_line}`,
      content: source.content,
    })
  }

  function openSearchResultSource(result: DocumentSearchResult) {
    setSourcePreview({
      title: result.file_name,
      path: result.path,
      lineRange: `Lines ${result.start_line}-${result.end_line}`,
      content: result.content,
    })
  }

  async function refreshModelLabData(showLoading = true) {
    if (!isTauriRuntime()) {
      setModelLabError('Model Lab requires the Tauri desktop app.')
      return
    }

    try {
      if (showLoading) {
        setIsModelLabLoading(true)
      }
      const [benchmarks, usage] = await Promise.all([
        listModelBenchmarks(100),
        listModelUsage(),
      ])
      setModelBenchmarks(benchmarks)
      setModelUsage(usage)
      setModelLabError(null)
    } catch (error) {
      setModelLabError(String(error))
    } finally {
      if (showLoading) {
        setIsModelLabLoading(false)
      }
    }
  }

  function openModelLab() {
    setIsModelLabOpen(true)
    void refreshModelLabData()
  }

  async function handleStartModelBenchmark(model: string) {
    if (!isTauriRuntime()) {
      setModelLabError('Model Lab requires the Tauri desktop app.')
      return
    }

    try {
      setModelLabAction(model)
      const job = await startModelBenchmark(model)
      setJobs((currentJobs) => upsertJob(currentJobs, job))
      await refreshModelLabData(false)
      setModelLabError(null)
    } catch (error) {
      const details = String(error)
      if (details !== 'Job cancelled') {
        setModelLabError(details)
      }
      await refreshModelLabData(false)
    } finally {
      setModelLabAction(null)
    }
  }

  async function handleDownloadModel(model: string) {
    if (!isTauriRuntime()) {
      setModelError({
        message: 'Model downloads require the Tauri desktop app.',
      })
      return
    }

    try {
      setModelAction(`download:${model}`)
      const job = await downloadOllamaModel(model)
      setJobs((currentJobs) => upsertJob(currentJobs, job))
      applyOllamaStatus(await getOllamaStatus(model), model)
      setModelError(null)
    } catch (error) {
      const details = String(error)
      if (details === 'Job cancelled') {
        setModelError(null)
      } else {
        setModelError({
          message: 'Could not download model.',
          details,
        })
      }
    } finally {
      setModelAction(null)
    }
  }

  async function handleCancelJob(jobId: string) {
    if (!isTauriRuntime()) {
      return
    }

    try {
      const job = await cancelJob(jobId)
      setJobs((currentJobs) => upsertJob(currentJobs, job))
      setHistoryError(null)
    } catch (error) {
      setHistoryError(String(error))
    }
  }

  async function refreshDatabaseDiagnostics() {
    if (!isTauriRuntime()) {
      setDatabaseDiagnosticsError('Diagnostics require the Tauri desktop app.')
      return
    }

    try {
      setIsDatabaseDiagnosticsLoading(true)
      setDatabaseDiagnostics(await getDatabaseDiagnostics())
      setDatabaseDiagnosticsError(null)
    } catch (error) {
      setDatabaseDiagnosticsError(String(error))
    } finally {
      setIsDatabaseDiagnosticsLoading(false)
    }
  }

  function openDiagnostics() {
    setIsDiagnosticsOpen(true)
    void refreshDatabaseDiagnostics()
  }

  async function handleDeleteModel(model: string) {
    if (!isTauriRuntime()) {
      setModelError({
        message: 'Model deletion requires the Tauri desktop app.',
      })
      return
    }

    try {
      setModelAction(`delete:${model}`)
      const models = await deleteOllamaModel(model)
      const nextSelectedModel = selectedModel === model ? '' : selectedModel
      applyOllamaStatus(buildOllamaStatusFromModels(models, nextSelectedModel))
      setModelError(null)
    } catch (error) {
      setModelError({
        message: 'Could not delete model.',
        details: String(error),
      })
    } finally {
      setModelAction(null)
    }
  }

  function closeChatSearch() {
    setIsChatSearchOpen(false)
    setChatSearchQuery('')
    setChatSearchResults([])
    setChatSearchError(null)
    setIsChatSearchLoading(false)
  }

  function openChatSearch() {
    setIsSidebarOpen(true)
    setIsChatSearchOpen(true)
    setChatSearchError(null)
  }

  function handleToggleChatSearch() {
    if (isChatSearchOpen) {
      closeChatSearch()
      return
    }

    openChatSearch()
  }

  function handleChatSearchChange(value: string) {
    setChatSearchQuery(value)
    setChatSearchError(null)

    if (!value.trim()) {
      setChatSearchResults([])
      setIsChatSearchLoading(false)
      return
    }

    if (isTauriRuntime()) {
      setIsChatSearchLoading(true)
    }
  }

  function handleKnowledgeSearchChange(value: string) {
    setKnowledgeSearchQuery(value)

    if (!value.trim()) {
      setKnowledgeSearchResults([])
    }
  }

  async function handleNewChat() {
    setActiveChatId(null)
    setMessages([])
    setDraft('')
    setPendingSearchJump(null)
    setHighlightedMessageId(null)
    setIsExportMenuOpen(false)
    setIsSummaryPanelOpen(false)
    setActiveSummary(null)
    setSummaryDraft('')
    setSummaryError(null)
    setMemoryPromptSetting(null)
    setKnowledgePromptSetting(null)
    closeChatSearch()
  }

  function openModelManager() {
    setIsModelPanelOpen(true)
  }

  async function handleSelectChat(chatId: string) {
    setActiveChatId(chatId)
    setActiveSummary(null)
    setSummaryDraft('')
    setSummaryError(null)
    setMemoryPromptSetting(null)
    setKnowledgePromptSetting(null)
    setIsExportMenuOpen(false)
    setPendingSearchJump(null)
    closeChatSearch()
  }

  function handleSelectSearchResult(result: ChatSearchResult) {
    setActiveChatId(result.chat_id)
    setActiveSummary(null)
    setSummaryDraft('')
    setSummaryError(null)
    setMemoryPromptSetting(null)
    setKnowledgePromptSetting(null)
    setIsExportMenuOpen(false)
    setPendingSearchJump(
      result.message_id === null
        ? null
        : { chatId: result.chat_id, messageId: result.message_id },
    )
    closeChatSearch()
  }

  function handleSelectModel(model: string) {
    setSelectedModel(model)
    setOllamaStatus((currentStatus) =>
      currentStatus ? buildOllamaStatusFromModels(currentStatus.models, model) : currentStatus,
    )
    setIsModelPanelOpen(false)
    setHistoryError(null)
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()

    if (isResponding) {
      return
    }

    const content = draft.trim()
    if (!content) {
      return
    }

    if (!isTauriRuntime()) {
      const message: ChatMessage = {
        id: Date.now(),
        chat_id: 'browser-preview',
        role: 'user',
        content,
        created_at: Date.now(),
        generation_run: null,
      }
      setMessages((currentMessages) => [...currentMessages, message])
      setDraft('')
      setHistoryError('Chat responses require the Tauri desktop app.')
      return
    }

    const chatBlockReason = getChatBlockReason(ollamaStatus, selectedModel)
    if (chatBlockReason) {
      setHistoryError(chatBlockReason)
      return
    }

    try {
      setIsResponding(true)
      let chatId = activeChatId

      if (!chatId) {
        const chat = await invoke<ChatSummary>('create_chat', {
          title: titleFromMessage(content),
        })
        chatId = chat.id
        upsertChat(chat)
      }

      setRespondingChatId(chatId)
      setActiveChatId(chatId)

      const userMessage = await invoke<ChatMessage>('add_message', {
        chatId,
        role: 'user',
        content,
      })

      setMessages((currentMessages) => [...currentMessages, userMessage])
      setDraft('')
      setHistoryError(null)
      await refreshChats()

      const assistantMessage = await invoke<ChatMessage>(
        'generate_assistant_response',
        {
          chatId,
          model: selectedModel,
        },
      )

      if (activeChatIdRef.current === chatId) {
        setMessages((currentMessages) => [...currentMessages, assistantMessage])
      }

      const [loadedMessages] = await Promise.all([
        invoke<ChatMessage[]>('get_messages', { chatId }),
        refreshChats(),
      ])

      if (activeChatIdRef.current === chatId) {
        setMessages(loadedMessages)
      }
    } catch (error) {
      const message = String(error)
      setHistoryError(message === 'Generation cancelled' ? null : message)
    } finally {
      setIsResponding(false)
      setRespondingChatId(null)
    }
  }

  async function handleCancelResponse() {
    if (!respondingChatId || !isTauriRuntime()) {
      setIsResponding(false)
      setRespondingChatId(null)
      return
    }

    try {
      await invoke<boolean>('cancel_ollama_generation', {
        chatId: respondingChatId,
      })
      setHistoryError(null)
    } catch (error) {
      setHistoryError(String(error))
    } finally {
      setIsResponding(false)
      setRespondingChatId(null)
    }
  }

  async function handleDeleteActiveChat() {
    if (!activeChatId || !isTauriRuntime()) {
      return
    }

    const chatId = activeChatId

    try {
      setDeletingChatId(chatId)

      if (respondingChatId === chatId) {
        await invoke<boolean>('cancel_ollama_generation', { chatId })
      }

      await invoke<boolean>('delete_chat', { chatId })
      setChats((currentChats) =>
        currentChats.filter((chat) => chat.id !== chatId),
      )
      setActiveChatId(null)
      setMessages([])
      setDraft('')
      setIsExportMenuOpen(false)
      setIsSummaryPanelOpen(false)
      setActiveSummary(null)
      setSummaryDraft('')
      setSummaryError(null)
      setMemoryPromptSetting(null)
      setKnowledgePromptSetting(null)
      setHistoryError(null)

      if (respondingChatId === chatId) {
        setIsResponding(false)
        setRespondingChatId(null)
      }
    } catch (error) {
      setHistoryError(String(error))
    } finally {
      setDeletingChatId(null)
    }
  }

  async function handleExportActiveChat(format: ChatExportFormat) {
    if (!activeChatId || !isTauriRuntime()) {
      setHistoryError(
        activeChatId
          ? 'Chat export requires the Tauri desktop app.'
          : 'Open a chat before exporting.',
      )
      return
    }

    try {
      setExportAction(format)
      const exportedChat = await exportChat(activeChatId, format)
      saveChatExport(exportedChat)
      setIsExportMenuOpen(false)
      setHistoryError(null)
    } catch (error) {
      setHistoryError(String(error))
    } finally {
      setExportAction(null)
    }
  }

  const normalizedChatSearch = chatSearchQuery.trim().toLowerCase()
  const hasChatSearch = isChatSearchOpen && normalizedChatSearch.length > 0
  const hasDesktopChatSearch = hasChatSearch && isTauriRuntime()
  const visibleChats = hasChatSearch
    ? isTauriRuntime()
      ? []
      : chats.filter((chat) =>
          chat.title.toLowerCase().includes(normalizedChatSearch),
        )
    : chats
  const activeChatIsResponding =
    isResponding && activeChatId !== null && respondingChatId === activeChatId
  const readinessNotice = getReadinessNotice(
    ollamaStatus,
    selectedModel,
    isTauriRuntime(),
  )
  const modelPanelEmptyText = getModelPanelEmptyText(ollamaStatus)
  const visibleJobs = jobs.filter(
    (job) => isActiveJob(job) || job.status === 'failed',
  )
  const activeModelPullJob = jobs.find(
    (job) => job.job_type === 'model_pull' && isActiveJob(job),
  )
  const activeModelBenchmarkJob = jobs.find(
    (job) => job.job_type === 'model_benchmark' && isActiveJob(job),
  )
  const activeModelBenchmarkProgress = activeModelBenchmarkJob
    ? getJobProgressPercent(activeModelBenchmarkJob)
    : null
  const activeConversationSummaryJob = activeChatId
    ? jobs.find(
        (job) =>
          job.job_type === 'conversation_summary' &&
          isActiveJob(job) &&
          getJobChatId(job) === activeChatId,
      )
    : undefined
  const activeConversationSummaryProgress = activeConversationSummaryJob
    ? getJobProgressPercent(activeConversationSummaryJob)
    : null
  const activeKnowledgeIndexJob = jobs.find(
    (job) => job.job_type === 'document_import' && isActiveJob(job),
  )
  const activeKnowledgeIndexProgress = activeKnowledgeIndexJob
    ? getJobProgressPercent(activeKnowledgeIndexJob)
    : null
  const activePromptMemories =
    activeChatId && memoryPromptSetting?.enabled_for_prompt
      ? memories.filter(
          (memory) =>
            memory.archived_at === null &&
            (memory.scope_type === 'global' ||
              (memory.scope_type === 'conversation' &&
                memory.scope_id === activeChatId)),
        )
      : []
  const indexedKnowledgeDocumentCount = knowledgeWorkspaces.reduce(
    (sum, workspace) => sum + workspace.document_count,
    0,
  )
  const modelUsageByName = new Map(
    modelUsage.map((usage) => [usage.model_name, usage]),
  )
  const completedBenchmarks = modelBenchmarks.filter(
    (benchmark) =>
      benchmark.status === 'completed' && benchmark.tokens_per_second !== null,
  )
  const fastestBenchmark = completedBenchmarks.reduce<ModelBenchmark | null>(
    (fastest, benchmark) => {
      if (!fastest) {
        return benchmark
      }

      return (benchmark.tokens_per_second ?? 0) >
        (fastest.tokens_per_second ?? 0)
        ? benchmark
        : fastest
    },
    null,
  )
  const modelDownloadsDisabled =
    modelAction !== null ||
    activeModelPullJob !== undefined ||
    isOllamaStatusLoading ||
    !isTauriRuntime() ||
    ollamaStatus?.status === 'unavailable'
  const commandEntries = buildCommandRegistry({
    activeChatId,
    deletingChatId,
    exportAction,
    hasActiveConversationSummaryJob: activeConversationSummaryJob !== undefined,
    hasActiveKnowledgeIndexJob: activeKnowledgeIndexJob !== undefined,
    hasActiveModelBenchmarkJob: activeModelBenchmarkJob !== undefined,
    hasActiveModelPullJob: activeModelPullJob !== undefined,
    isDesktop: isTauriRuntime(),
    isOllamaStatusLoading,
    modelAction,
    ollamaModels,
    ollamaStatus,
    selectedModel,
    onDeleteActiveChat: handleDeleteActiveChat,
    onDownloadModel: handleDownloadModel,
    onExportChat: handleExportActiveChat,
    onNewChat: handleNewChat,
    onOpenChatSearch: openChatSearch,
    onOpenConversationSummary: openConversationSummary,
    onOpenDiagnostics: openDiagnostics,
    onOpenKnowledgeWorkspace: openKnowledgeWorkspace,
    onOpenMemoryInspector: openMemoryInspector,
    onOpenModelLab: openModelLab,
    onOpenModelManager: openModelManager,
    onRefreshModels: refreshOllamaModels,
    onSelectModel: handleSelectModel,
  })
  const visibleCommandEntries = filterCommands(commandEntries, commandQuery)
  const activeVisibleCommandIndex =
    visibleCommandEntries.length > 0
      ? Math.min(activeCommandIndex, visibleCommandEntries.length - 1)
      : -1

  function closeCommandPalette() {
    setIsCommandPaletteOpen(false)
    setCommandQuery('')
    setActiveCommandIndex(0)
  }

  function runCommand(command: AppCommand | undefined) {
    if (!command || command.disabledReason) {
      return
    }

    closeCommandPalette()
    void Promise.resolve(command.run()).catch((error: unknown) => {
      setHistoryError(String(error))
    })
  }

  function handleCommandPaletteKeyDown(
    event: KeyboardEvent<HTMLInputElement>,
  ) {
    if (event.key === 'ArrowDown') {
      event.preventDefault()
      if (visibleCommandEntries.length > 0) {
        setActiveCommandIndex(
          (currentIndex) => (currentIndex + 1) % visibleCommandEntries.length,
        )
      }
      return
    }

    if (event.key === 'ArrowUp') {
      event.preventDefault()
      if (visibleCommandEntries.length > 0) {
        setActiveCommandIndex(
          (currentIndex) =>
            (currentIndex - 1 + visibleCommandEntries.length) %
            visibleCommandEntries.length,
        )
      }
      return
    }

    if (event.key === 'Enter') {
      event.preventDefault()
      runCommand(visibleCommandEntries[activeVisibleCommandIndex])
      return
    }

    if (event.key === 'Escape') {
      event.preventDefault()
      closeCommandPalette()
    }
  }

  return (
    <>
      <div className="flex min-h-svh w-full overflow-hidden bg-black text-zinc-50">
      <aside
        className={`min-h-svh flex-none overflow-hidden border-r border-zinc-800/80 px-4 py-4 transition-[width] duration-200 ${
          isSidebarOpen ? 'w-64' : 'w-18'
        }`}
        aria-label="Chat navigation"
      >
        <div
          className={`flex items-center ${
            isSidebarOpen
              ? 'justify-between gap-4'
              : 'flex-col justify-center gap-3'
          } ${
            isSidebarOpen ? 'mb-7 md:mb-10' : 'mb-0'
          }`}
        >
          {isSidebarOpen ? (
            <div className="flex min-w-0 items-center gap-2.5">
              <ShipWheelLogo className="h-7 w-7 flex-none text-zinc-100" />
              <h1 className="truncate text-[22px] leading-none font-semibold tracking-[-0.01em]">
                Atlas
              </h1>
            </div>
          ) : (
            <div className="grid h-10 w-10 place-items-center text-zinc-100">
              <ShipWheelLogo className="h-7 w-7" />
            </div>
          )}

          <button
            className="grid h-9 w-9 place-items-center rounded-xl border-0 bg-transparent p-0 text-zinc-400 transition-colors hover:bg-zinc-900 hover:text-zinc-100"
            type="button"
            aria-expanded={isSidebarOpen}
            aria-label={isSidebarOpen ? 'Collapse sidebar' : 'Expand sidebar'}
            onClick={() => setIsSidebarOpen((open) => !open)}
          >
            <SidebarToggleIcon />
          </button>
        </div>

        {isSidebarOpen ? (
          <nav className="grid min-w-0 gap-2">
            <button
              className="flex min-h-11 w-full min-w-0 items-center gap-3 overflow-hidden rounded-xl border-0 bg-zinc-900 px-3 text-left text-base leading-none text-zinc-50 transition-colors hover:bg-zinc-800"
              type="button"
              onClick={handleNewChat}
            >
              <NewChatIcon className="h-5.5 w-5.5 flex-none" />
              <span className="min-w-0 truncate">New chat</span>
            </button>

            <button
              className={`flex min-h-11 w-full min-w-0 items-center gap-3 overflow-hidden rounded-xl border-0 px-3 text-left text-base leading-none text-zinc-50 transition-colors ${
                isChatSearchOpen
                  ? 'bg-zinc-900'
                  : 'bg-transparent hover:bg-zinc-900'
              }`}
              type="button"
              aria-expanded={isChatSearchOpen}
              aria-controls="chat-search"
              onClick={handleToggleChatSearch}
            >
              <SearchIcon className="h-5.5 w-5.5 flex-none" />
              <span className="min-w-0 truncate">Search chats</span>
            </button>

            {isChatSearchOpen ? (
              <div className="relative min-w-0">
                <label className="sr-only" htmlFor="chat-search">
                  Search chats
                </label>
                <SearchIcon className="pointer-events-none absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-zinc-500" />
                <input
                  className="h-10 w-full rounded-xl border border-zinc-800 bg-zinc-950 pr-10 pl-9 text-sm text-zinc-100 outline-none transition-colors placeholder:text-zinc-600 focus:border-zinc-600"
                  id="chat-search"
                  type="search"
                  placeholder="Search saved chats"
                  value={chatSearchQuery}
                  autoFocus
                  onChange={(event) =>
                    handleChatSearchChange(event.target.value)
                  }
                />
                <button
                  className="absolute top-1/2 right-2 grid h-7 w-7 -translate-y-1/2 place-items-center rounded-lg border-0 bg-transparent text-zinc-500 transition-colors hover:bg-zinc-900 hover:text-zinc-100"
                  type="button"
                  aria-label="Close chat search"
                  onClick={closeChatSearch}
                >
                  <XIcon />
                </button>
              </div>
            ) : null}

            <div className="mt-4 grid min-w-0 gap-1 border-t border-zinc-800/80 pt-4">
              {chatSearchError ? (
                <p className="mb-2 rounded-lg border border-red-900/60 bg-red-950/30 px-3 py-2 text-xs text-red-200">
                  {chatSearchError}
                </p>
              ) : null}

              {hasDesktopChatSearch ? (
                chatSearchResults.length > 0 ? (
                  chatSearchResults.map((result) => (
                    <button
                      className={`min-w-0 overflow-hidden rounded-lg border-0 px-3 py-2 text-left text-sm transition-colors ${
                        result.chat_id === activeChatId
                          ? 'bg-zinc-800 text-zinc-50'
                          : 'bg-transparent text-zinc-300 hover:bg-zinc-900 hover:text-zinc-50'
                      }`}
                      key={`${result.source}:${result.chat_id}:${result.message_id ?? 'title'}`}
                      type="button"
                      onClick={() => handleSelectSearchResult(result)}
                    >
                      <span className="block truncate font-medium">
                        {result.chat_title}
                      </span>
                      <span className="mt-1 block text-xs text-zinc-500">
                        {searchResultSourceLabel(result)} -{' '}
                        {formatSearchTimestamp(result.created_at)} -{' '}
                        {result.message_count} message
                        {result.message_count === 1 ? '' : 's'}
                      </span>
                      <span className="mt-1 block max-h-10 overflow-hidden text-xs leading-5 text-zinc-400">
                        {result.snippet.map((part, index) => (
                          <span
                            className={
                              part.is_match
                                ? 'rounded bg-zinc-700 px-0.5 text-zinc-50'
                                : undefined
                            }
                            key={`${index}:${part.text}`}
                          >
                            {part.text}
                          </span>
                        ))}
                      </span>
                    </button>
                  ))
                ) : (
                  <p className="px-3 text-sm text-zinc-500">
                    {isChatSearchLoading ? 'Searching...' : 'No matching chats'}
                  </p>
                )
              ) : visibleChats.length > 0 ? (
                visibleChats.map((chat) => (
                  <button
                    className={`min-h-10 min-w-0 overflow-hidden rounded-lg border-0 px-3 text-left text-sm transition-colors ${
                      chat.id === activeChatId
                        ? 'bg-zinc-800 text-zinc-50'
                        : 'bg-transparent text-zinc-300 hover:bg-zinc-900 hover:text-zinc-50'
                    }`}
                    key={chat.id}
                    type="button"
                    onClick={() => handleSelectChat(chat.id)}
                  >
                    <span className="block truncate">{chat.title}</span>
                    <span className="mt-1 block text-xs text-zinc-500">
                      {chat.message_count} message
                      {chat.message_count === 1 ? '' : 's'}
                    </span>
                  </button>
                ))
              ) : (
                <p className="px-3 text-sm text-zinc-500">
                  {hasChatSearch
                    ? isChatSearchLoading
                      ? 'Searching...'
                      : 'No matching chats'
                    : 'No saved chats yet'}
                </p>
              )}
            </div>
          </nav>
        ) : null}
      </aside>

      <main className="relative min-h-svh min-w-0 flex-1 overflow-hidden" aria-label="Chat">
        {activeChatId ? (
          <div className="absolute top-4 right-4 z-10 flex items-start gap-2">
            <button
              className={`grid h-10 w-10 place-items-center rounded-xl border shadow-[0_12px_36px_rgba(0,0,0,0.35)] transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${
                knowledgePromptSetting?.enabled_for_prompt
                  ? 'border-zinc-500 bg-zinc-100 text-zinc-950 hover:bg-white'
                  : 'border-zinc-800 bg-black/90 text-zinc-300 hover:bg-zinc-950 hover:text-zinc-100'
              }`}
              type="button"
              aria-label={
                knowledgePromptSetting?.enabled_for_prompt
                  ? 'Open Knowledge Workspace, knowledge use enabled'
                  : 'Open Knowledge Workspace'
              }
              onClick={openKnowledgeWorkspace}
            >
              <KnowledgeIcon />
            </button>

            <button
              className={`grid h-10 w-10 place-items-center rounded-xl border shadow-[0_12px_36px_rgba(0,0,0,0.35)] transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${
                memoryPromptSetting?.enabled_for_prompt
                  ? 'border-zinc-500 bg-zinc-100 text-zinc-950 hover:bg-white'
                  : 'border-zinc-800 bg-black/90 text-zinc-300 hover:bg-zinc-950 hover:text-zinc-100'
              }`}
              type="button"
              aria-label={
                memoryPromptSetting?.enabled_for_prompt
                  ? 'Open Memory Inspector, memory use enabled'
                  : 'Open Memory Inspector'
              }
              onClick={openMemoryInspector}
            >
              <MemoryIcon />
            </button>

            <button
              className={`grid h-10 w-10 place-items-center rounded-xl border shadow-[0_12px_36px_rgba(0,0,0,0.35)] transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${
                activeSummary?.enabled_for_prompt
                  ? 'border-zinc-500 bg-zinc-100 text-zinc-950 hover:bg-white'
                  : 'border-zinc-800 bg-black/90 text-zinc-300 hover:bg-zinc-950 hover:text-zinc-100'
              }`}
              type="button"
              aria-label={
                activeSummary?.enabled_for_prompt
                  ? 'Open chat summary, prompt use enabled'
                  : 'Open chat summary'
              }
              onClick={openConversationSummary}
            >
              <SummaryIcon />
            </button>

            <div className="relative">
              <button
                className="grid h-10 w-10 place-items-center rounded-xl border border-zinc-800 bg-black/90 text-zinc-300 shadow-[0_12px_36px_rgba(0,0,0,0.35)] transition-colors hover:bg-zinc-950 hover:text-zinc-100 disabled:cursor-not-allowed disabled:opacity-50"
                type="button"
                aria-label="Export chat"
                aria-expanded={isExportMenuOpen}
                aria-controls="chat-export-menu"
                disabled={exportAction !== null}
                onClick={() => setIsExportMenuOpen((isOpen) => !isOpen)}
              >
                <DownloadIcon />
              </button>

              {isExportMenuOpen ? (
                <div
                  className="absolute top-12 right-0 w-44 overflow-hidden rounded-xl border border-zinc-800 bg-zinc-950 p-1 shadow-[0_18px_50px_rgba(0,0,0,0.45)]"
                  id="chat-export-menu"
                >
                  {chatExportFormats.map((format) => (
                    <button
                      className="flex min-h-10 w-full items-center justify-between gap-3 rounded-lg border-0 bg-transparent px-3 text-left text-sm text-zinc-200 transition-colors hover:bg-zinc-900 disabled:cursor-not-allowed disabled:text-zinc-600"
                      key={format.format}
                      type="button"
                      disabled={exportAction !== null}
                      onClick={() => handleExportActiveChat(format.format)}
                    >
                      <span className="truncate">{format.label}</span>
                      {exportAction === format.format ? (
                        <span className="text-xs text-zinc-500">Saving</span>
                      ) : null}
                    </button>
                  ))}
                </div>
              ) : null}
            </div>

            <button
              className="grid h-10 w-10 place-items-center rounded-xl border border-red-950/80 bg-black/90 text-red-300 shadow-[0_12px_36px_rgba(0,0,0,0.35)] transition-colors hover:bg-red-950/30 hover:text-red-100 disabled:cursor-not-allowed disabled:opacity-50"
              type="button"
              aria-label="Delete chat"
              disabled={deletingChatId === activeChatId}
              onClick={handleDeleteActiveChat}
            >
              <TrashIcon />
            </button>
          </div>
        ) : null}

        {!activeChatId ? (
          <div className="absolute top-4 right-4 z-10">
            <button
              className="grid h-10 w-10 place-items-center rounded-xl border border-zinc-800 bg-black/90 text-zinc-300 shadow-[0_12px_36px_rgba(0,0,0,0.35)] transition-colors hover:bg-zinc-950 hover:text-zinc-100"
              type="button"
              aria-label="Open Knowledge Workspace"
              onClick={openKnowledgeWorkspace}
            >
              <KnowledgeIcon />
            </button>
          </div>
        ) : null}

        <div className="absolute top-4 left-4 z-10 w-[min(360px,calc(100%-2rem))]">
          {isModelPanelOpen ? (
            <section
              className="rounded-2xl border border-zinc-800 bg-black/95 p-3 shadow-[0_18px_50px_rgba(0,0,0,0.45)]"
              aria-label="Model manager"
            >
              <header className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="text-xs font-semibold tracking-[0.08em] text-zinc-500 uppercase">
                    Model
                  </p>
                  <p className="mt-1 truncate text-sm font-medium text-zinc-100">
                    {selectedModel || 'Select a local model'}
                  </p>
                </div>
                <div className="flex flex-none items-center gap-1">
                  <button
                    className="h-8 rounded-lg border border-zinc-800 bg-zinc-950 px-2.5 text-xs font-medium text-zinc-300 transition-colors hover:bg-zinc-900 disabled:cursor-not-allowed disabled:opacity-50"
                    type="button"
                    disabled={modelAction !== null || isOllamaStatusLoading}
                    onClick={refreshOllamaModels}
                  >
                    {modelAction === 'refresh' ? 'Refreshing' : 'Refresh'}
                  </button>
                  <button
                    className="h-8 rounded-lg border border-zinc-800 bg-zinc-950 px-2.5 text-xs font-medium text-zinc-300 transition-colors hover:bg-zinc-900"
                    type="button"
                    onClick={openModelLab}
                  >
                    Lab
                  </button>
                  <button
                    className="grid h-8 w-8 place-items-center rounded-lg border border-zinc-800 bg-zinc-950 text-zinc-400 transition-colors hover:bg-zinc-900 hover:text-zinc-100"
                    type="button"
                    aria-label="Close model manager"
                    onClick={() => setIsModelPanelOpen(false)}
                  >
                    <XIcon />
                  </button>
                </div>
              </header>

              {isOllamaStatusLoading ? (
                <p className="mt-3 rounded-xl bg-zinc-950 px-3 py-2 text-xs text-zinc-500">
                  Checking Ollama...
                </p>
              ) : null}

              {ollamaStatus?.status === 'selected_model_missing' ? (
                <p className="mt-3 rounded-xl border border-amber-900/60 bg-amber-950/20 px-3 py-2 text-xs text-amber-100">
                  Selected model is not installed.
                </p>
              ) : null}

              <div className="mt-3 grid max-h-48 gap-1 overflow-y-auto border-t border-zinc-800/80 pt-3">
                {ollamaModels.length > 0 ? (
                  ollamaModels.map((model) => (
                    <div
                      className={`flex items-center gap-2 rounded-xl px-2 py-2 ${
                        selectedModel === model.name
                          ? 'bg-zinc-900'
                          : 'hover:bg-zinc-950'
                      }`}
                      key={model.name}
                    >
                      <button
                        className="min-w-0 flex-1 border-0 bg-transparent p-0 text-left"
                        type="button"
                        onClick={() => handleSelectModel(model.name)}
                      >
                        <span className="block truncate text-sm text-zinc-100">
                          {model.name}
                        </span>
                        <span className="mt-0.5 block text-xs text-zinc-600">
                          {formatModelSize(model.size)}
                        </span>
                      </button>
                      {selectedModel === model.name ? (
                        <span className="rounded-full bg-zinc-100 px-2 py-0.5 text-[11px] font-semibold text-zinc-950">
                          Active
                        </span>
                      ) : null}
                      <button
                        className="rounded-lg border border-red-950/80 px-2 py-1 text-xs font-medium text-red-300 transition-colors hover:bg-red-950/30 disabled:cursor-not-allowed disabled:opacity-50"
                        type="button"
                        disabled={modelAction !== null}
                        onClick={() => handleDeleteModel(model.name)}
                      >
                        {modelAction === `delete:${model.name}`
                          ? 'Deleting'
                          : 'Delete'}
                      </button>
                    </div>
                  ))
                ) : (
                  <p className="rounded-xl bg-zinc-950 px-3 py-2 text-sm text-zinc-500">
                    {modelPanelEmptyText}
                  </p>
                )}
              </div>

              <div className="mt-3 border-t border-zinc-800/80 pt-3">
                <p className="mb-2 text-xs font-semibold tracking-[0.08em] text-zinc-500 uppercase">
                  Recommended Downloads
                </p>
                <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                  {recommendedModels.map((model) => {
                    const installed = ollamaModels.some(
                      (installedModel) => installedModel.name === model.name,
                    )
                    const isDownloading =
                      modelAction === `download:${model.name}`

                    return (
                      <div
                        className="rounded-xl bg-zinc-950 px-3 py-2"
                        key={model.name}
                      >
                        <p className="truncate text-sm font-medium text-zinc-100">
                          {model.name}
                        </p>
                        <div className="mt-2 flex items-center justify-between gap-2">
                          <span className="text-xs text-zinc-500">
                            {model.note}
                          </span>
                          <button
                            className="rounded-lg bg-zinc-100 px-2.5 py-1 text-xs font-semibold text-zinc-950 transition-colors hover:bg-white disabled:cursor-not-allowed disabled:bg-zinc-800 disabled:text-zinc-500"
                            type="button"
                            disabled={
                              installed ||
                              modelDownloadsDisabled
                            }
                            onClick={() => handleDownloadModel(model.name)}
                          >
                            {installed
                              ? 'Installed'
                              : isDownloading
                                ? 'Downloading'
                                : 'Get'}
                          </button>
                        </div>
                      </div>
                    )
                  })}
                </div>
              </div>

              {!isTauriRuntime() ? (
                <p className="mt-3 text-xs text-zinc-500">
                  Model management requires the Tauri desktop app.
                </p>
              ) : null}

              {modelError ? (
                <div className="mt-3 rounded-xl border border-red-900/60 bg-red-950/30 px-3 py-2 text-xs text-red-200">
                  <p>{modelError.message}</p>
                  {modelError.details ? (
                    <details className="mt-2">
                      <summary className="cursor-pointer text-red-100">
                        Details
                      </summary>
                      <p className="mt-1 break-words text-red-200/80">
                        {modelError.details}
                      </p>
                    </details>
                  ) : null}
                </div>
              ) : null}
            </section>
          ) : (
            <button
              className="flex h-10 max-w-full min-w-0 items-center gap-2 overflow-hidden rounded-xl border border-zinc-800 bg-black/90 px-3 text-left text-sm text-zinc-200 shadow-[0_12px_36px_rgba(0,0,0,0.35)] transition-colors hover:bg-zinc-950"
              type="button"
              onClick={() => setIsModelPanelOpen(true)}
            >
              <span className="flex-none text-xs font-semibold tracking-[0.08em] text-zinc-500 uppercase">
                Model
              </span>
              <span className="min-w-0 truncate font-medium">
                {selectedModel || 'Select model'}
              </span>
            </button>
          )}
        </div>

        <div className="absolute inset-x-0 top-20 bottom-28 overflow-y-auto px-4">
          <div className="mx-auto flex max-w-3xl flex-col gap-4">
            {readinessNotice ? (
              <section className="mr-auto max-w-[min(100%,34rem)] rounded-2xl border border-zinc-800 bg-zinc-950 px-4 py-3 text-sm text-zinc-200">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="font-medium text-zinc-100">
                      {readinessNotice.title}
                    </p>
                    <p className="mt-1 text-zinc-500">{readinessNotice.body}</p>
                  </div>
                  <div className="flex flex-none items-center gap-2">
                    {isTauriRuntime() ? (
                      <button
                        className="rounded-lg border border-zinc-700 px-2.5 py-1 text-xs font-medium text-zinc-200 transition-colors hover:bg-zinc-900 disabled:cursor-not-allowed disabled:opacity-50"
                        type="button"
                        disabled={modelAction !== null || isOllamaStatusLoading}
                        onClick={refreshOllamaModels}
                      >
                        Retry
                      </button>
                    ) : null}
                    {readinessNotice.title !== 'Ollama is offline' ? (
                      <button
                        className="rounded-lg bg-zinc-100 px-2.5 py-1 text-xs font-semibold text-zinc-950 transition-colors hover:bg-white"
                        type="button"
                        onClick={() => setIsModelPanelOpen(true)}
                      >
                        Models
                      </button>
                    ) : null}
                  </div>
                </div>

                {readinessNotice.details ? (
                  <details className="mt-3 text-xs text-zinc-500">
                    <summary className="cursor-pointer text-zinc-400">
                      Details
                    </summary>
                    <p className="mt-1 break-words">{readinessNotice.details}</p>
                  </details>
                ) : null}
              </section>
            ) : null}

            {activeSummary?.enabled_for_prompt ? (
              <section className="mr-auto flex max-w-[min(100%,34rem)] flex-wrap items-center justify-between gap-3 rounded-2xl border border-zinc-800 bg-zinc-950 px-4 py-3 text-sm text-zinc-200">
                <div className="min-w-0">
                  <p className="font-medium text-zinc-100">
                    Summary context on
                  </p>
                  <p className="mt-1 text-xs text-zinc-500">
                    {formatSummaryRange(activeSummary)} - v{activeSummary.version}
                  </p>
                </div>
                <button
                  className="rounded-lg border border-zinc-700 px-2.5 py-1 text-xs font-medium text-zinc-200 transition-colors hover:bg-zinc-900"
                  type="button"
                  onClick={openConversationSummary}
                >
                  Edit
                </button>
              </section>
            ) : null}

            {memoryPromptSetting?.enabled_for_prompt ? (
              <section className="mr-auto flex max-w-[min(100%,34rem)] flex-wrap items-center justify-between gap-3 rounded-2xl border border-zinc-800 bg-zinc-950 px-4 py-3 text-sm text-zinc-200">
                <div className="min-w-0">
                  <p className="font-medium text-zinc-100">
                    Memory context on
                  </p>
                  <p className="mt-1 text-xs text-zinc-500">
                    {`${activePromptMemories.length} active ${
                      activePromptMemories.length === 1 ? 'memory' : 'memories'
                    }`}
                  </p>
                </div>
                <button
                  className="rounded-lg border border-zinc-700 px-2.5 py-1 text-xs font-medium text-zinc-200 transition-colors hover:bg-zinc-900"
                  type="button"
                  onClick={openMemoryInspector}
                >
                  Inspect
                </button>
              </section>
            ) : null}

            {knowledgePromptSetting?.enabled_for_prompt ? (
              <section className="mr-auto flex max-w-[min(100%,34rem)] flex-wrap items-center justify-between gap-3 rounded-2xl border border-zinc-800 bg-zinc-950 px-4 py-3 text-sm text-zinc-200">
                <div className="min-w-0">
                  <p className="font-medium text-zinc-100">
                    Knowledge context on
                  </p>
                  <p className="mt-1 text-xs text-zinc-500">
                    {`${indexedKnowledgeDocumentCount} indexed ${
                      indexedKnowledgeDocumentCount === 1
                        ? 'document'
                        : 'documents'
                    }`}
                  </p>
                </div>
                <button
                  className="rounded-lg border border-zinc-700 px-2.5 py-1 text-xs font-medium text-zinc-200 transition-colors hover:bg-zinc-900"
                  type="button"
                  onClick={openKnowledgeWorkspace}
                >
                  Inspect
                </button>
              </section>
            ) : null}

            {messages.map((message) => (
              <article
                className={`max-w-[78%] overflow-hidden rounded-2xl px-4 py-3 text-sm leading-6 break-words transition-[box-shadow,outline-color] ${
                  message.role === 'user'
                    ? 'ml-auto bg-zinc-100 text-zinc-950'
                    : 'mr-auto bg-zinc-900 text-zinc-100'
                } ${
                  highlightedMessageId === message.id
                    ? 'outline outline-2 outline-zinc-400'
                    : 'outline outline-0 outline-transparent'
                }`}
                key={message.id}
                ref={(element) => {
                  if (element) {
                    messageRefs.current.set(message.id, element)
                    return
                  }

                  messageRefs.current.delete(message.id)
                }}
              >
                <div>{message.content}</div>
                {isTauriRuntime() ? (
                  <div className="mt-3 flex justify-end">
                    <button
                      className={`rounded-lg border px-2 py-1 text-xs font-medium transition-colors ${
                        message.role === 'user'
                          ? 'border-zinc-300 text-zinc-700 hover:bg-zinc-200'
                          : 'border-zinc-700 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100'
                      }`}
                      type="button"
                      onClick={() => handleRememberMessage(message)}
                    >
                      Remember
                    </button>
                  </div>
                ) : null}
                {message.role === 'assistant' ? (
                  <MessageDiagnostics
                    run={message.generation_run}
                    onOpenSource={openGenerationSource}
                  />
                ) : null}
              </article>
            ))}

            {activeChatIsResponding ? (
              <article className="mr-auto flex max-w-[78%] items-center gap-1 rounded-2xl bg-zinc-900 px-4 py-3 text-sm leading-6 text-zinc-400">
                <span className="sr-only">Atlas is thinking</span>
                <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-zinc-500 [animation-delay:-0.2s]" />
                <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-zinc-500 [animation-delay:-0.1s]" />
                <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-zinc-500" />
              </article>
            ) : null}

            {historyError ? (
              <p className="rounded-xl border border-red-900/60 bg-red-950/30 px-4 py-3 text-sm text-red-200">
                {historyError}
              </p>
            ) : null}
          </div>
        </div>

        <form
          className="absolute bottom-4 left-1/2 flex min-h-14 w-[calc(100%-2rem)] max-w-[760px] -translate-x-1/2 items-center gap-3 rounded-[1.75rem] border border-zinc-700/70 bg-zinc-900 px-4 shadow-[0_12px_40px_rgba(0,0,0,0.35)] md:bottom-6 md:min-h-[58px] md:px-[18px]"
          onSubmit={handleSubmit}
        >
          <label className="sr-only" htmlFor="chat-input">
            Message
          </label>
          <input
            className="w-full min-w-0 border-0 bg-transparent text-lg leading-tight text-zinc-100 outline-none placeholder:text-zinc-400"
            id="chat-input"
            type="text"
            placeholder={isResponding ? 'Atlas is responding...' : 'Ask anything'}
            value={draft}
            disabled={isResponding}
            onChange={(event) => setDraft(event.target.value)}
          />
          {isResponding ? (
            <button
              className="h-9 flex-none rounded-full border border-red-950/80 px-4 text-sm font-semibold text-red-200 transition-colors hover:bg-red-950/40"
              type="button"
              onClick={handleCancelResponse}
            >
              Cancel
            </button>
          ) : (
            <button
              className="h-9 flex-none rounded-full bg-zinc-100 px-4 text-sm font-semibold text-zinc-950 transition-colors hover:bg-white disabled:cursor-not-allowed disabled:bg-zinc-800 disabled:text-zinc-500"
              type="submit"
              disabled={!draft.trim()}
            >
              Send
            </button>
          )}
        </form>
      </main>
      </div>

      {visibleJobs.length > 0 ? (
        <section
          className="fixed right-4 bottom-24 z-30 grid w-[min(24rem,calc(100%-2rem))] gap-2"
          aria-label="Jobs"
        >
          {visibleJobs.slice(0, 3).map((job) => {
            const progressPercent = getJobProgressPercent(job)

            return (
              <div
                className={`rounded-xl border bg-zinc-950/95 px-3 py-2 shadow-[0_14px_44px_rgba(0,0,0,0.45)] ${
                  job.status === 'failed'
                    ? 'border-red-900/70'
                    : 'border-zinc-800'
                }`}
                key={job.id}
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="truncate text-sm font-medium text-zinc-100">
                      {job.label}
                    </p>
                    <p className="mt-0.5 text-xs text-zinc-500">
                      {formatJobProgress(job)}
                    </p>
                  </div>
                  <div className="flex flex-none items-center gap-2">
                    <span
                      className={`rounded-lg px-2 py-1 text-[11px] font-medium ${
                        job.status === 'failed'
                          ? 'bg-red-950/50 text-red-200'
                          : 'bg-zinc-900 text-zinc-400'
                      }`}
                    >
                      {jobStatusLabel(job.status)}
                    </span>
                    {isActiveJob(job) ? (
                      <button
                        className="rounded-lg border border-zinc-800 px-2 py-1 text-xs font-medium text-zinc-300 transition-colors hover:bg-zinc-900 disabled:cursor-not-allowed disabled:opacity-50"
                        type="button"
                        disabled={job.status === 'cancelling'}
                        onClick={() => handleCancelJob(job.id)}
                      >
                        Cancel
                      </button>
                    ) : null}
                  </div>
                </div>

                {progressPercent !== null ? (
                  <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-zinc-900">
                    <div
                      className="h-full rounded-full bg-zinc-100 transition-[width]"
                      style={{ width: `${progressPercent}%` }}
                    />
                  </div>
                ) : null}

                {job.status === 'failed' && job.error_message ? (
                  <p className="mt-2 break-words text-xs text-red-200/90">
                    {job.error_message}
                  </p>
                ) : null}
              </div>
            )
          })}
        </section>
      ) : null}

      {isSummaryPanelOpen ? (
        <div
          className="fixed inset-0 z-50 bg-black/70 px-4 py-[8vh]"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) {
              setIsSummaryPanelOpen(false)
            }
          }}
        >
          <section
            className="mx-auto flex max-h-[84vh] w-full max-w-3xl flex-col overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-950 shadow-[0_24px_80px_rgba(0,0,0,0.55)]"
            role="dialog"
            aria-modal="true"
            aria-label="Conversation summary"
          >
            <header className="flex items-start justify-between gap-4 border-b border-zinc-800 px-4 py-3">
              <div className="min-w-0">
                <p className="text-sm font-semibold text-zinc-100">
                  Conversation Summary
                </p>
                <p className="mt-0.5 text-xs text-zinc-500">
                  {activeSummary
                    ? `${formatSummaryRange(activeSummary)} - ${activeSummary.model_name}`
                    : 'No summary saved'}
                </p>
              </div>
              <button
                className="grid h-8 w-8 place-items-center rounded-lg border-0 bg-transparent text-zinc-500 transition-colors hover:bg-zinc-900 hover:text-zinc-100"
                type="button"
                aria-label="Close conversation summary"
                onClick={() => setIsSummaryPanelOpen(false)}
              >
                <XIcon />
              </button>
            </header>

            <div className="grid gap-4 overflow-y-auto p-4">
              {summaryError ? (
                <p className="rounded-xl border border-red-900/60 bg-red-950/30 px-3 py-2 text-sm text-red-200">
                  {summaryError}
                </p>
              ) : null}

              {activeConversationSummaryJob ? (
                <div className="rounded-xl border border-zinc-800 bg-zinc-900/60 px-3 py-2">
                  <div className="flex items-center justify-between gap-3">
                    <div className="min-w-0">
                      <p className="truncate text-sm font-medium text-zinc-100">
                        {activeConversationSummaryJob.label}
                      </p>
                      <p className="mt-0.5 text-xs text-zinc-500">
                        {formatJobProgress(activeConversationSummaryJob)}
                      </p>
                    </div>
                    <button
                      className="rounded-lg border border-zinc-700 px-2.5 py-1 text-xs font-medium text-zinc-300 transition-colors hover:bg-zinc-800 disabled:cursor-not-allowed disabled:opacity-50"
                      type="button"
                      disabled={activeConversationSummaryJob.status === 'cancelling'}
                      onClick={() =>
                        handleCancelJob(activeConversationSummaryJob.id)
                      }
                    >
                      Cancel
                    </button>
                  </div>
                  {activeConversationSummaryProgress !== null ? (
                    <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-zinc-950">
                      <div
                        className="h-full rounded-full bg-zinc-100 transition-[width]"
                        style={{
                          width: `${activeConversationSummaryProgress}%`,
                        }}
                      />
                    </div>
                  ) : null}
                </div>
              ) : null}

              <div className="grid gap-2 sm:grid-cols-3">
                <div className="rounded-xl bg-zinc-900/70 px-3 py-2">
                  <p className="text-xs text-zinc-600">Version</p>
                  <p className="mt-1 text-sm text-zinc-100">
                    {activeSummary ? `v${activeSummary.version}` : 'None'}
                  </p>
                </div>
                <div className="rounded-xl bg-zinc-900/70 px-3 py-2">
                  <p className="text-xs text-zinc-600">Updated</p>
                  <p className="mt-1 truncate text-sm text-zinc-100">
                    {formatTimestamp(activeSummary?.updated_at)}
                  </p>
                </div>
                <div className="rounded-xl bg-zinc-900/70 px-3 py-2">
                  <p className="text-xs text-zinc-600">Model</p>
                  <p className="mt-1 truncate text-sm text-zinc-100">
                    {activeSummary?.model_name ?? 'None'}
                  </p>
                </div>
              </div>

              <label className="flex items-center justify-between gap-3 rounded-xl border border-zinc-800 bg-zinc-900/50 px-3 py-2 text-sm text-zinc-200">
                <span className="min-w-0">
                  <span className="block font-medium text-zinc-100">
                    Use in prompts
                  </span>
                  <span className="mt-0.5 block text-xs text-zinc-500">
                    {activeSummary
                      ? activeSummary.enabled_for_prompt
                        ? 'Enabled for this chat'
                        : 'Disabled for this chat'
                      : 'Save a summary first'}
                  </span>
                </span>
                <input
                  className="h-5 w-5 flex-none accent-zinc-100"
                  type="checkbox"
                  checked={activeSummary?.enabled_for_prompt ?? false}
                  disabled={
                    !activeSummary ||
                    summaryAction !== null ||
                    activeConversationSummaryJob !== undefined
                  }
                  onChange={(event) =>
                    void handleToggleSummaryUse(event.target.checked)
                  }
                />
              </label>

              <div>
                <label
                  className="mb-2 block text-xs font-semibold tracking-[0.08em] text-zinc-500 uppercase"
                  htmlFor="conversation-summary"
                >
                  Summary
                </label>
                <textarea
                  className="min-h-72 w-full resize-y rounded-xl border border-zinc-800 bg-zinc-950 px-3 py-3 text-sm leading-6 text-zinc-100 outline-none transition-colors placeholder:text-zinc-600 focus:border-zinc-600"
                  id="conversation-summary"
                  placeholder={
                    isSummaryLoading
                      ? 'Loading summary...'
                      : 'Generate or write a summary for this conversation.'
                  }
                  value={summaryDraft}
                  onChange={(event) => setSummaryDraft(event.target.value)}
                />
              </div>

              <div className="flex flex-wrap items-center justify-between gap-2">
                <button
                  className="h-9 rounded-lg bg-zinc-100 px-3 text-sm font-semibold text-zinc-950 transition-colors hover:bg-white disabled:cursor-not-allowed disabled:bg-zinc-800 disabled:text-zinc-500"
                  type="button"
                  disabled={
                    summaryAction !== null ||
                    activeConversationSummaryJob !== undefined ||
                    isResponding ||
                    !selectedModel
                  }
                  onClick={handleGenerateSummary}
                >
                  {summaryAction === 'generate'
                    ? 'Generating'
                    : activeSummary
                      ? 'Update summary'
                      : 'Generate summary'}
                </button>

                <div className="flex flex-wrap items-center gap-2">
                  <button
                    className="h-9 rounded-lg border border-zinc-800 px-3 text-sm font-medium text-zinc-300 transition-colors hover:bg-zinc-900 disabled:cursor-not-allowed disabled:opacity-50"
                    type="button"
                    disabled={
                      summaryAction !== null ||
                      activeConversationSummaryJob !== undefined ||
                      !summaryDraft.trim()
                    }
                    onClick={handleSaveSummary}
                  >
                    {summaryAction === 'save' ? 'Saving' : 'Save edits'}
                  </button>
                  <button
                    className="h-9 rounded-lg border border-red-950/80 px-3 text-sm font-medium text-red-300 transition-colors hover:bg-red-950/30 disabled:cursor-not-allowed disabled:opacity-50"
                    type="button"
                    disabled={
                      !activeSummary ||
                      summaryAction !== null ||
                      activeConversationSummaryJob !== undefined
                    }
                    onClick={handleDeleteSummary}
                  >
                    {summaryAction === 'delete' ? 'Deleting' : 'Delete'}
                  </button>
                </div>
              </div>
            </div>
          </section>
        </div>
      ) : null}

      {isMemoryInspectorOpen ? (
        <div
          className="fixed inset-0 z-50 bg-black/70 px-4 py-[6vh]"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) {
              setIsMemoryInspectorOpen(false)
            }
          }}
        >
          <section
            className="mx-auto flex max-h-[88vh] w-full max-w-5xl flex-col overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-950 shadow-[0_24px_80px_rgba(0,0,0,0.55)]"
            role="dialog"
            aria-modal="true"
            aria-label="Memory Inspector"
          >
            <header className="flex items-start justify-between gap-4 border-b border-zinc-800 px-4 py-3">
              <div className="min-w-0">
                <p className="text-sm font-semibold text-zinc-100">
                  Memory Inspector
                </p>
                <p className="mt-0.5 text-xs text-zinc-500">
                  {`${memories.length} stored ${
                    memories.length === 1 ? 'memory' : 'memories'
                  }`}
                </p>
              </div>
              <div className="flex flex-none items-center gap-2">
                <button
                  className="h-8 rounded-lg border border-zinc-800 px-2.5 text-xs font-medium text-zinc-300 transition-colors hover:bg-zinc-900 disabled:cursor-not-allowed disabled:opacity-50"
                  type="button"
                  disabled={isMemoryLoading}
                  onClick={() => void refreshMemoryData()}
                >
                  {isMemoryLoading ? 'Loading' : 'Refresh'}
                </button>
                <button
                  className="grid h-8 w-8 place-items-center rounded-lg border-0 bg-transparent text-zinc-500 transition-colors hover:bg-zinc-900 hover:text-zinc-100"
                  type="button"
                  aria-label="Close Memory Inspector"
                  onClick={() => setIsMemoryInspectorOpen(false)}
                >
                  <XIcon />
                </button>
              </div>
            </header>

            <div className="grid gap-4 overflow-y-auto p-4 lg:grid-cols-[minmax(0,0.9fr)_minmax(0,1.25fr)]">
              <section className="min-w-0">
                {memoryError ? (
                  <p className="mb-3 rounded-xl border border-red-900/60 bg-red-950/30 px-3 py-2 text-sm text-red-200">
                    {memoryError}
                  </p>
                ) : null}

                <label className="flex items-center justify-between gap-3 rounded-xl border border-zinc-800 bg-zinc-900/50 px-3 py-2 text-sm text-zinc-200">
                  <span className="min-w-0">
                    <span className="block font-medium text-zinc-100">
                      Use memories in this chat
                    </span>
                    <span className="mt-0.5 block text-xs text-zinc-500">
                      {activeChatId
                        ? memoryPromptSetting?.enabled_for_prompt
                          ? 'Enabled'
                          : 'Disabled'
                        : 'No active chat'}
                    </span>
                  </span>
                  <input
                    className="h-5 w-5 flex-none accent-zinc-100"
                    type="checkbox"
                    checked={memoryPromptSetting?.enabled_for_prompt ?? false}
                    disabled={!activeChatId || memoryAction === 'toggle-prompt'}
                    onChange={(event) =>
                      void handleSetMemoryPromptEnabled(event.target.checked)
                    }
                  />
                </label>

                <div className="mt-4 rounded-xl border border-zinc-800 bg-zinc-900/40 p-3">
                  <div className="flex items-center justify-between gap-3">
                    <p className="text-xs font-semibold tracking-[0.08em] text-zinc-500 uppercase">
                      {editingMemoryId ? 'Edit Memory' : 'New Memory'}
                    </p>
                    {editingMemoryId || memorySource ? (
                      <button
                        className="rounded-lg border border-zinc-800 px-2 py-1 text-xs font-medium text-zinc-300 transition-colors hover:bg-zinc-900"
                        type="button"
                        onClick={resetMemoryForm}
                      >
                        Clear
                      </button>
                    ) : null}
                  </div>

                  <label className="mt-3 block text-xs text-zinc-500" htmlFor="memory-scope">
                    Scope
                  </label>
                  <select
                    className="mt-1 h-10 w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 text-sm text-zinc-100 outline-none focus:border-zinc-600 disabled:opacity-50"
                    id="memory-scope"
                    value={memoryScope}
                    disabled={editingMemoryId !== null}
                    onChange={(event) =>
                      setMemoryScope(event.target.value as MemoryScopeType)
                    }
                  >
                    <option value="global">Global</option>
                    <option value="conversation" disabled={!activeChatId}>
                      Conversation
                    </option>
                  </select>

                  <label className="mt-3 block text-xs text-zinc-500" htmlFor="memory-content">
                    Content
                  </label>
                  <textarea
                    className="mt-1 min-h-36 w-full resize-y rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 text-sm leading-6 text-zinc-100 outline-none transition-colors placeholder:text-zinc-600 focus:border-zinc-600"
                    id="memory-content"
                    placeholder="Write a memory."
                    value={memoryDraft}
                    onChange={(event) => setMemoryDraft(event.target.value)}
                  />

                  {memorySource ? (
                    <p className="mt-2 rounded-lg bg-zinc-950 px-2 py-1.5 text-xs text-zinc-500">
                      Source: {memorySource.label}
                    </p>
                  ) : null}

                  <label className="mt-3 flex items-center gap-2 text-sm text-zinc-300">
                    <input
                      className="h-4 w-4 accent-zinc-100"
                      type="checkbox"
                      checked={memoryPinned}
                      onChange={(event) => setMemoryPinned(event.target.checked)}
                    />
                    <span>Pin</span>
                  </label>

                  <button
                    className="mt-3 h-9 w-full rounded-lg bg-zinc-100 px-3 text-sm font-semibold text-zinc-950 transition-colors hover:bg-white disabled:cursor-not-allowed disabled:bg-zinc-800 disabled:text-zinc-500"
                    type="button"
                    disabled={!memoryDraft.trim() || memoryAction !== null}
                    onClick={handleSaveMemory}
                  >
                    {memoryAction === 'create' ||
                    (editingMemoryId && memoryAction === `save:${editingMemoryId}`)
                      ? 'Saving'
                      : editingMemoryId
                        ? 'Save memory'
                        : 'Add memory'}
                  </button>
                </div>
              </section>

              <section className="min-w-0">
                <p className="mb-2 text-xs font-semibold tracking-[0.08em] text-zinc-500 uppercase">
                  Stored Memories
                </p>
                <div className="grid gap-2">
                  {memories.length > 0 ? (
                    memories.map((memory) => (
                      <div
                        className={`rounded-xl border px-3 py-2 ${
                          memory.archived_at
                            ? 'border-zinc-900 bg-zinc-950/60 opacity-70'
                            : 'border-zinc-800 bg-zinc-900/70'
                        }`}
                        key={memory.id}
                      >
                        <div className="flex items-start justify-between gap-3">
                          <div className="min-w-0">
                            <p className="break-words text-sm leading-6 text-zinc-100">
                              {memory.content}
                            </p>
                            <p className="mt-1 text-xs text-zinc-500">
                              {formatMemoryScope(memory)} - {formatMemorySource(memory)}
                              {memory.pinned ? ' - pinned' : ''}
                              {memory.archived_at ? ' - archived' : ''}
                            </p>
                          </div>
                        </div>

                        <div className="mt-3 flex flex-wrap items-center gap-2">
                          <button
                            className="rounded-lg border border-zinc-800 px-2 py-1 text-xs font-medium text-zinc-300 transition-colors hover:bg-zinc-900 disabled:cursor-not-allowed disabled:opacity-50"
                            type="button"
                            disabled={memoryAction !== null}
                            onClick={() => handleEditMemory(memory)}
                          >
                            Edit
                          </button>
                          {memory.source_conversation_id ? (
                            <button
                              className="rounded-lg border border-zinc-800 px-2 py-1 text-xs font-medium text-zinc-300 transition-colors hover:bg-zinc-900"
                              type="button"
                              onClick={() => handleOpenMemorySource(memory)}
                            >
                              Source
                            </button>
                          ) : null}
                          <button
                            className="rounded-lg border border-zinc-800 px-2 py-1 text-xs font-medium text-zinc-300 transition-colors hover:bg-zinc-900 disabled:cursor-not-allowed disabled:opacity-50"
                            type="button"
                            disabled={memoryAction !== null}
                            onClick={() => handleArchiveMemory(memory)}
                          >
                            {memoryAction === `archive:${memory.id}`
                              ? 'Working'
                              : memory.archived_at
                                ? 'Restore'
                                : 'Archive'}
                          </button>
                          <button
                            className="rounded-lg border border-red-950/80 px-2 py-1 text-xs font-medium text-red-300 transition-colors hover:bg-red-950/30 disabled:cursor-not-allowed disabled:opacity-50"
                            type="button"
                            disabled={memoryAction !== null}
                            onClick={() => handleDeleteMemory(memory)}
                          >
                            {memoryAction === `delete:${memory.id}`
                              ? 'Forgetting'
                              : 'Forget'}
                          </button>
                        </div>
                      </div>
                    ))
                  ) : (
                    <p className="rounded-xl bg-zinc-900/70 px-3 py-2 text-sm text-zinc-500">
                      No memories stored.
                    </p>
                  )}
                </div>
              </section>
            </div>
          </section>
        </div>
      ) : null}

      {isKnowledgeWorkspaceOpen ? (
        <div
          className="fixed inset-0 z-50 bg-black/70 px-4 py-[6vh]"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) {
              setIsKnowledgeWorkspaceOpen(false)
            }
          }}
        >
          <section
            className="mx-auto flex max-h-[88vh] w-full max-w-5xl flex-col overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-950 shadow-[0_24px_80px_rgba(0,0,0,0.55)]"
            role="dialog"
            aria-modal="true"
            aria-label="Knowledge Workspace"
          >
            <header className="flex items-start justify-between gap-4 border-b border-zinc-800 px-4 py-3">
              <div className="min-w-0">
                <p className="text-sm font-semibold text-zinc-100">
                  Knowledge Workspace
                </p>
                <p className="mt-0.5 text-xs text-zinc-500">
                  {`${knowledgeWorkspaces.length} ${
                    knowledgeWorkspaces.length === 1 ? 'workspace' : 'workspaces'
                  } - ${indexedKnowledgeDocumentCount} indexed ${
                    indexedKnowledgeDocumentCount === 1 ? 'document' : 'documents'
                  }`}
                </p>
              </div>
              <div className="flex flex-none items-center gap-2">
                <button
                  className="h-8 rounded-lg border border-zinc-800 px-2.5 text-xs font-medium text-zinc-300 transition-colors hover:bg-zinc-900 disabled:cursor-not-allowed disabled:opacity-50"
                  type="button"
                  disabled={isKnowledgeLoading}
                  onClick={() => void refreshKnowledgeData()}
                >
                  {isKnowledgeLoading ? 'Loading' : 'Refresh'}
                </button>
                <button
                  className="grid h-8 w-8 place-items-center rounded-lg border-0 bg-transparent text-zinc-500 transition-colors hover:bg-zinc-900 hover:text-zinc-100"
                  type="button"
                  aria-label="Close Knowledge Workspace"
                  onClick={() => setIsKnowledgeWorkspaceOpen(false)}
                >
                  <XIcon />
                </button>
              </div>
            </header>

            <div className="grid gap-4 overflow-y-auto p-4 lg:grid-cols-[minmax(0,0.9fr)_minmax(0,1.25fr)]">
              <section className="min-w-0">
                {knowledgeError ? (
                  <p className="mb-3 rounded-xl border border-red-900/60 bg-red-950/30 px-3 py-2 text-sm text-red-200">
                    {knowledgeError}
                  </p>
                ) : null}

                <label className="flex items-center justify-between gap-3 rounded-xl border border-zinc-800 bg-zinc-900/50 px-3 py-2 text-sm text-zinc-200">
                  <span className="min-w-0">
                    <span className="block font-medium text-zinc-100">
                      Use knowledge in this chat
                    </span>
                    <span className="mt-0.5 block text-xs text-zinc-500">
                      {activeChatId
                        ? knowledgePromptSetting?.enabled_for_prompt
                          ? 'Enabled'
                          : 'Disabled'
                        : 'No active chat'}
                    </span>
                  </span>
                  <input
                    className="h-5 w-5 flex-none accent-zinc-100"
                    type="checkbox"
                    checked={knowledgePromptSetting?.enabled_for_prompt ?? false}
                    disabled={!activeChatId || knowledgeAction === 'toggle-prompt'}
                    onChange={(event) =>
                      void handleSetKnowledgePromptEnabled(event.target.checked)
                    }
                  />
                </label>

                {activeKnowledgeIndexJob ? (
                  <div className="mt-4 rounded-xl border border-zinc-800 bg-zinc-900/60 px-3 py-2">
                    <div className="flex items-center justify-between gap-3">
                      <div className="min-w-0">
                        <p className="truncate text-sm font-medium text-zinc-100">
                          {activeKnowledgeIndexJob.label}
                        </p>
                        <p className="mt-0.5 text-xs text-zinc-500">
                          {formatJobProgress(activeKnowledgeIndexJob)}
                        </p>
                      </div>
                      <button
                        className="rounded-lg border border-zinc-700 px-2.5 py-1 text-xs font-medium text-zinc-300 transition-colors hover:bg-zinc-800 disabled:cursor-not-allowed disabled:opacity-50"
                        type="button"
                        disabled={activeKnowledgeIndexJob.status === 'cancelling'}
                        onClick={() => handleCancelJob(activeKnowledgeIndexJob.id)}
                      >
                        Cancel
                      </button>
                    </div>
                    {activeKnowledgeIndexProgress !== null ? (
                      <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-zinc-950">
                        <div
                          className="h-full rounded-full bg-zinc-100 transition-[width]"
                          style={{ width: `${activeKnowledgeIndexProgress}%` }}
                        />
                      </div>
                    ) : null}
                  </div>
                ) : null}

                <div className="mt-4 rounded-xl border border-zinc-800 bg-zinc-900/40 p-3">
                  <label
                    className="block text-xs font-semibold tracking-[0.08em] text-zinc-500 uppercase"
                    htmlFor="knowledge-path"
                  >
                    Path
                  </label>
                  <div className="mt-2 flex gap-2">
                    <input
                      className="h-10 min-w-0 flex-1 rounded-lg border border-zinc-800 bg-zinc-950 px-3 text-sm text-zinc-100 outline-none transition-colors placeholder:text-zinc-600 focus:border-zinc-600"
                      id="knowledge-path"
                      type="text"
                      placeholder="/Users/rehanislam/project"
                      value={knowledgePath}
                      onChange={(event) => setKnowledgePath(event.target.value)}
                    />
                    <button
                      className="h-10 rounded-lg bg-zinc-100 px-3 text-sm font-semibold text-zinc-950 transition-colors hover:bg-white disabled:cursor-not-allowed disabled:bg-zinc-800 disabled:text-zinc-500"
                      type="button"
                      disabled={
                        !knowledgePath.trim() ||
                        knowledgeAction !== null ||
                        activeKnowledgeIndexJob !== undefined
                      }
                      onClick={handleIndexKnowledgePath}
                    >
                      {knowledgeAction === 'index' ? 'Indexing' : 'Index'}
                    </button>
                  </div>
                </div>

                <section className="mt-4 min-w-0">
                  <p className="mb-2 text-xs font-semibold tracking-[0.08em] text-zinc-500 uppercase">
                    Indexed Workspaces
                  </p>
                  <div className="grid gap-2">
                    {knowledgeWorkspaces.length > 0 ? (
                      knowledgeWorkspaces.map((workspace) => (
                        <div
                          className="rounded-xl border border-zinc-800 bg-zinc-900/70 px-3 py-2"
                          key={workspace.id}
                        >
                          <div className="flex items-start justify-between gap-3">
                            <div className="min-w-0">
                              <p className="truncate text-sm font-medium text-zinc-100">
                                {workspace.name}
                              </p>
                              <p className="mt-1 truncate text-xs text-zinc-500">
                                {workspace.root_path}
                              </p>
                              <p className="mt-1 text-xs text-zinc-500">
                                {workspace.document_count} docs -{' '}
                                {workspace.chunk_count} chunks
                              </p>
                            </div>
                            <button
                              className="rounded-lg border border-red-950/80 px-2 py-1 text-xs font-medium text-red-300 transition-colors hover:bg-red-950/30 disabled:cursor-not-allowed disabled:opacity-50"
                              type="button"
                              disabled={knowledgeAction !== null}
                              onClick={() =>
                                handleRemoveKnowledgeWorkspace(workspace)
                              }
                            >
                              {knowledgeAction === `remove:${workspace.id}`
                                ? 'Removing'
                                : 'Remove'}
                            </button>
                          </div>
                        </div>
                      ))
                    ) : (
                      <p className="rounded-xl bg-zinc-900/70 px-3 py-2 text-sm text-zinc-500">
                        No indexed workspaces.
                      </p>
                    )}
                  </div>
                </section>
              </section>

              <section className="min-w-0">
                <label
                  className="mb-2 block text-xs font-semibold tracking-[0.08em] text-zinc-500 uppercase"
                  htmlFor="knowledge-search"
                >
                  Search
                </label>
                <input
                  className="h-10 w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 text-sm text-zinc-100 outline-none transition-colors placeholder:text-zinc-600 focus:border-zinc-600"
                  id="knowledge-search"
                  type="search"
                  placeholder="Search indexed files"
                  value={knowledgeSearchQuery}
                  onChange={(event) =>
                    handleKnowledgeSearchChange(event.target.value)
                  }
                />

                <div className="mt-3 grid gap-2">
                  {knowledgeSearchQuery.trim() ? (
                    knowledgeSearchResults.length > 0 ? (
                      knowledgeSearchResults.map((result) => (
                        <button
                          className="rounded-xl border border-zinc-800 bg-zinc-900/70 px-3 py-2 text-left transition-colors hover:bg-zinc-900"
                          key={result.chunk_id}
                          type="button"
                          onClick={() => openSearchResultSource(result)}
                        >
                          <span className="block truncate text-sm font-medium text-zinc-100">
                            {result.file_name}
                          </span>
                          <span className="mt-1 block text-xs text-zinc-500">
                            Lines {result.start_line}-{result.end_line}
                          </span>
                          <span className="mt-1 block max-h-12 overflow-hidden text-xs leading-5 text-zinc-400">
                            {result.snippet}
                          </span>
                        </button>
                      ))
                    ) : (
                      <p className="rounded-xl bg-zinc-900/70 px-3 py-2 text-sm text-zinc-500">
                        No matching chunks.
                      </p>
                    )
                  ) : knowledgeDocuments.length > 0 ? (
                    knowledgeDocuments.map((document) => (
                      <div
                        className="rounded-xl border border-zinc-800 bg-zinc-900/70 px-3 py-2"
                        key={document.id}
                      >
                        <p className="truncate text-sm font-medium text-zinc-100">
                          {document.file_name}
                        </p>
                        <p className="mt-1 truncate text-xs text-zinc-500">
                          {document.path}
                        </p>
                        <p className="mt-1 text-xs text-zinc-500">
                          {document.chunk_count} chunks -{' '}
                          {formatBytes(document.size_bytes)}
                        </p>
                      </div>
                    ))
                  ) : (
                    <p className="rounded-xl bg-zinc-900/70 px-3 py-2 text-sm text-zinc-500">
                      No indexed documents.
                    </p>
                  )}
                </div>
              </section>
            </div>
          </section>
        </div>
      ) : null}

      {sourcePreview ? (
        <div
          className="fixed inset-0 z-[60] bg-black/70 px-4 py-[8vh]"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) {
              setSourcePreview(null)
            }
          }}
        >
          <section
            className="mx-auto flex max-h-[84vh] w-full max-w-3xl flex-col overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-950 shadow-[0_24px_80px_rgba(0,0,0,0.55)]"
            role="dialog"
            aria-modal="true"
            aria-label="Source chunk"
          >
            <header className="flex items-start justify-between gap-4 border-b border-zinc-800 px-4 py-3">
              <div className="min-w-0">
                <p className="truncate text-sm font-semibold text-zinc-100">
                  {sourcePreview.title}
                </p>
                <p className="mt-0.5 truncate text-xs text-zinc-500">
                  {sourcePreview.path}
                </p>
                <p className="mt-0.5 text-xs text-zinc-500">
                  {sourcePreview.lineRange}
                </p>
              </div>
              <button
                className="grid h-8 w-8 place-items-center rounded-lg border-0 bg-transparent text-zinc-500 transition-colors hover:bg-zinc-900 hover:text-zinc-100"
                type="button"
                aria-label="Close source chunk"
                onClick={() => setSourcePreview(null)}
              >
                <XIcon />
              </button>
            </header>
            <pre className="overflow-auto p-4 text-sm leading-6 whitespace-pre-wrap text-zinc-200">
              {sourcePreview.content}
            </pre>
          </section>
        </div>
      ) : null}

      {isModelLabOpen ? (
        <div
          className="fixed inset-0 z-50 bg-black/70 px-4 py-[6vh]"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) {
              setIsModelLabOpen(false)
            }
          }}
        >
          <section
            className="mx-auto flex max-h-[88vh] w-full max-w-5xl flex-col overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-950 shadow-[0_24px_80px_rgba(0,0,0,0.55)]"
            role="dialog"
            aria-modal="true"
            aria-label="Model Lab"
          >
            <header className="flex items-start justify-between gap-4 border-b border-zinc-800 px-4 py-3">
              <div className="min-w-0">
                <p className="text-sm font-semibold text-zinc-100">Model Lab</p>
                <p className="mt-0.5 text-xs text-zinc-500">
                  Speed and latency benchmarks for installed local models
                </p>
              </div>
              <div className="flex flex-none items-center gap-2">
                <button
                  className="h-8 rounded-lg border border-zinc-800 px-2.5 text-xs font-medium text-zinc-300 transition-colors hover:bg-zinc-900 disabled:cursor-not-allowed disabled:opacity-50"
                  type="button"
                  disabled={isModelLabLoading}
                  onClick={() => void refreshModelLabData()}
                >
                  {isModelLabLoading ? 'Loading' : 'Refresh'}
                </button>
                <button
                  className="grid h-8 w-8 place-items-center rounded-lg border-0 bg-transparent text-zinc-500 transition-colors hover:bg-zinc-900 hover:text-zinc-100"
                  type="button"
                  aria-label="Close Model Lab"
                  onClick={() => setIsModelLabOpen(false)}
                >
                  <XIcon />
                </button>
              </div>
            </header>

            <div className="overflow-y-auto p-4">
              {modelLabError ? (
                <p className="mb-3 rounded-xl border border-red-900/60 bg-red-950/30 px-3 py-2 text-sm text-red-200">
                  {modelLabError}
                </p>
              ) : null}

              {activeModelBenchmarkJob ? (
                <div className="mb-4 rounded-xl border border-zinc-800 bg-zinc-900/60 px-3 py-2">
                  <div className="flex items-center justify-between gap-3">
                    <div className="min-w-0">
                      <p className="truncate text-sm font-medium text-zinc-100">
                        {activeModelBenchmarkJob.label}
                      </p>
                      <p className="mt-0.5 text-xs text-zinc-500">
                        {formatJobProgress(activeModelBenchmarkJob)}
                      </p>
                    </div>
                    <button
                      className="rounded-lg border border-zinc-700 px-2.5 py-1 text-xs font-medium text-zinc-300 transition-colors hover:bg-zinc-800 disabled:cursor-not-allowed disabled:opacity-50"
                      type="button"
                      disabled={activeModelBenchmarkJob.status === 'cancelling'}
                      onClick={() => handleCancelJob(activeModelBenchmarkJob.id)}
                    >
                      Cancel
                    </button>
                  </div>
                  {activeModelBenchmarkProgress !== null ? (
                    <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-zinc-950">
                      <div
                        className="h-full rounded-full bg-zinc-100 transition-[width]"
                        style={{
                          width: `${activeModelBenchmarkProgress}%`,
                        }}
                      />
                    </div>
                  ) : null}
                </div>
              ) : null}

              <div className="mb-4 rounded-xl bg-zinc-900/70 px-3 py-2">
                <p className="text-xs text-zinc-500">Fastest measured model</p>
                <p className="mt-1 text-sm text-zinc-100">
                  {fastestBenchmark
                    ? `${fastestBenchmark.model_name} - ${formatSpeed(
                        fastestBenchmark.tokens_per_second,
                      )}`
                    : 'No completed benchmark yet'}
                </p>
              </div>

              <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.25fr)]">
                <section className="min-w-0">
                  <p className="mb-2 text-xs font-semibold tracking-[0.08em] text-zinc-500 uppercase">
                    Installed Models
                  </p>
                  <div className="grid gap-2">
                    {ollamaModels.length > 0 ? (
                      ollamaModels.map((model) => {
                        const usage = modelUsageByName.get(model.name)
                        const modelBenchmarkRows = modelBenchmarks.filter(
                          (benchmark) => benchmark.model_name === model.name,
                        )
                        const modelCompletedRows = modelBenchmarkRows.filter(
                          (benchmark) =>
                            benchmark.status === 'completed' &&
                            benchmark.tokens_per_second !== null,
                        )
                        const fastestModelRow = modelCompletedRows.reduce<
                          ModelBenchmark | null
                        >((fastest, benchmark) => {
                          if (!fastest) {
                            return benchmark
                          }

                          return (benchmark.tokens_per_second ?? 0) >
                            (fastest.tokens_per_second ?? 0)
                            ? benchmark
                            : fastest
                        }, null)
                        const benchmarkDisabled =
                          activeModelBenchmarkJob !== undefined ||
                          modelLabAction !== null ||
                          isOllamaStatusLoading ||
                          ollamaStatus?.status === 'unavailable'

                        return (
                          <div
                            className="rounded-xl bg-zinc-900/70 px-3 py-2"
                            key={model.name}
                          >
                            <div className="flex items-start justify-between gap-3">
                              <div className="min-w-0">
                                <p className="truncate text-sm font-medium text-zinc-100">
                                  {model.name}
                                </p>
                                <p className="mt-0.5 text-xs text-zinc-500">
                                  {formatModelSize(model.size)}
                                </p>
                              </div>
                              {fastestBenchmark?.model_name === model.name ? (
                                <span className="rounded-full bg-zinc-100 px-2 py-0.5 text-[11px] font-semibold text-zinc-950">
                                  Fastest measured
                                </span>
                              ) : null}
                            </div>
                            <dl className="mt-3 grid grid-cols-2 gap-2 text-xs">
                              <div>
                                <dt className="text-zinc-600">Last used</dt>
                                <dd className="mt-0.5 truncate text-zinc-300">
                                  {formatTimestamp(usage?.last_used_at)}
                                </dd>
                              </div>
                              <div>
                                <dt className="text-zinc-600">Runs</dt>
                                <dd className="mt-0.5 text-zinc-300">
                                  {usage?.generation_count ?? 0}
                                </dd>
                              </div>
                              <div>
                                <dt className="text-zinc-600">Best speed</dt>
                                <dd className="mt-0.5 text-zinc-300">
                                  {formatSpeed(fastestModelRow?.tokens_per_second)}
                                </dd>
                              </div>
                              <div>
                                <dt className="text-zinc-600">Benchmarks</dt>
                                <dd className="mt-0.5 text-zinc-300">
                                  {modelBenchmarkRows.length}
                                </dd>
                              </div>
                            </dl>
                            <button
                              className="mt-3 h-8 w-full rounded-lg bg-zinc-100 px-2.5 text-xs font-semibold text-zinc-950 transition-colors hover:bg-white disabled:cursor-not-allowed disabled:bg-zinc-800 disabled:text-zinc-500"
                              type="button"
                              disabled={benchmarkDisabled}
                              onClick={() => handleStartModelBenchmark(model.name)}
                            >
                              {modelLabAction === model.name
                                ? 'Benchmarking'
                                : 'Run benchmark'}
                            </button>
                          </div>
                        )
                      })
                    ) : (
                      <p className="rounded-xl bg-zinc-900/70 px-3 py-2 text-sm text-zinc-500">
                        No installed models found.
                      </p>
                    )}
                  </div>
                </section>

                <section className="min-w-0">
                  <p className="mb-2 text-xs font-semibold tracking-[0.08em] text-zinc-500 uppercase">
                    Benchmark History
                  </p>
                  {modelBenchmarks.length > 0 ? (
                    <div className="grid gap-2">
                      {modelBenchmarks.slice(0, 24).map((benchmark) => {
                        const promptSpeed = getEvalSpeed(
                          benchmark.prompt_eval_count,
                          benchmark.prompt_eval_duration_ms,
                        )

                        return (
                          <div
                            className="rounded-xl bg-zinc-900/70 px-3 py-2"
                            key={benchmark.id}
                          >
                            <div className="flex items-start justify-between gap-3">
                              <div className="min-w-0">
                                <p className="truncate text-sm font-medium text-zinc-100">
                                  {benchmark.model_name}
                                </p>
                                <p className="mt-0.5 text-xs text-zinc-500">
                                  {benchmark.prompt_label} -{' '}
                                  {formatBenchmarkStatus(benchmark.status)}
                                </p>
                              </div>
                              <span className="flex-none text-xs text-zinc-500">
                                {formatTimestamp(
                                  benchmark.completed_at ?? benchmark.started_at,
                                )}
                              </span>
                            </div>
                            <dl className="mt-3 grid grid-cols-2 gap-2 text-xs sm:grid-cols-4">
                              <div>
                                <dt className="text-zinc-600">Total</dt>
                                <dd className="mt-0.5 text-zinc-300">
                                  {formatDurationMs(benchmark.total_duration_ms) ??
                                    'n/a'}
                                </dd>
                              </div>
                              <div>
                                <dt className="text-zinc-600">First token</dt>
                                <dd className="mt-0.5 text-zinc-300">
                                  {formatDurationMs(benchmark.first_token_ms) ??
                                    'n/a'}
                                </dd>
                              </div>
                              <div>
                                <dt className="text-zinc-600">Prompt</dt>
                                <dd className="mt-0.5 text-zinc-300">
                                  {formatSpeed(promptSpeed)}
                                </dd>
                              </div>
                              <div>
                                <dt className="text-zinc-600">Completion</dt>
                                <dd className="mt-0.5 text-zinc-300">
                                  {formatSpeed(benchmark.tokens_per_second)}
                                </dd>
                              </div>
                            </dl>
                            {benchmark.error_message ? (
                              <p className="mt-2 break-words text-xs text-red-200/90">
                                {benchmark.error_message}
                              </p>
                            ) : null}
                          </div>
                        )
                      })}
                    </div>
                  ) : (
                    <p className="rounded-xl bg-zinc-900/70 px-3 py-2 text-sm text-zinc-500">
                      No benchmark history yet.
                    </p>
                  )}
                </section>
              </div>
            </div>
          </section>
        </div>
      ) : null}

      {isDiagnosticsOpen ? (
        <div
          className="fixed inset-0 z-50 bg-black/70 px-4 py-[10vh]"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) {
              setIsDiagnosticsOpen(false)
            }
          }}
        >
          <section
            className="mx-auto w-full max-w-2xl overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-950 shadow-[0_24px_80px_rgba(0,0,0,0.55)]"
            role="dialog"
            aria-modal="true"
            aria-label="Database diagnostics"
          >
            <header className="flex items-start justify-between gap-4 border-b border-zinc-800 px-4 py-3">
              <div className="min-w-0">
                <p className="text-sm font-semibold text-zinc-100">
                  Database diagnostics
                </p>
                <p className="mt-0.5 text-xs text-zinc-500">
                  SQLite storage and integrity
                </p>
              </div>
              <div className="flex flex-none items-center gap-2">
                <button
                  className="h-8 rounded-lg border border-zinc-800 px-2.5 text-xs font-medium text-zinc-300 transition-colors hover:bg-zinc-900 disabled:cursor-not-allowed disabled:opacity-50"
                  type="button"
                  disabled={isDatabaseDiagnosticsLoading}
                  onClick={refreshDatabaseDiagnostics}
                >
                  {isDatabaseDiagnosticsLoading ? 'Checking' : 'Refresh'}
                </button>
                <button
                  className="grid h-8 w-8 place-items-center rounded-lg border-0 bg-transparent text-zinc-500 transition-colors hover:bg-zinc-900 hover:text-zinc-100"
                  type="button"
                  aria-label="Close diagnostics"
                  onClick={() => setIsDiagnosticsOpen(false)}
                >
                  <XIcon />
                </button>
              </div>
            </header>

            <div className="max-h-[min(34rem,70vh)] overflow-y-auto p-4">
              {databaseDiagnosticsError ? (
                <p className="mb-3 rounded-xl border border-red-900/60 bg-red-950/30 px-3 py-2 text-sm text-red-200">
                  {databaseDiagnosticsError}
                </p>
              ) : null}

              {databaseDiagnostics ? (
                <div className="grid gap-4">
                  <div className="rounded-xl bg-zinc-900/70 px-3 py-2">
                    <p className="text-xs text-zinc-500">Path</p>
                    <p className="mt-1 break-all text-sm text-zinc-100">
                      {databaseDiagnostics.path}
                    </p>
                  </div>

                  <dl className="grid grid-cols-2 gap-2 sm:grid-cols-3">
                    {[
                      ['Database', formatBytes(databaseDiagnostics.database_size_bytes)],
                      ['WAL', formatBytes(databaseDiagnostics.wal_size_bytes)],
                      ['Shared memory', formatBytes(databaseDiagnostics.shm_size_bytes)],
                      ['Journal', databaseDiagnostics.journal_mode],
                      ['Schema', `v${databaseDiagnostics.user_version}`],
                      ['Integrity', databaseDiagnostics.integrity_check],
                      ['Pages', databaseDiagnostics.page_count.toLocaleString()],
                      ['Page size', formatBytes(databaseDiagnostics.page_size)],
                      ['Free pages', databaseDiagnostics.freelist_count.toLocaleString()],
                    ].map(([label, value]) => (
                      <div
                        className="min-w-0 rounded-xl bg-zinc-900/70 px-3 py-2"
                        key={label}
                      >
                        <dt className="text-xs text-zinc-500">{label}</dt>
                        <dd className="mt-1 truncate text-sm text-zinc-100">
                          {value}
                        </dd>
                      </div>
                    ))}
                  </dl>

                  <div>
                    <p className="mb-2 text-xs font-semibold tracking-[0.08em] text-zinc-500 uppercase">
                      Tables
                    </p>
                    <div className="grid gap-1">
                      {databaseDiagnostics.table_counts.map((table) => (
                        <div
                          className="flex items-center justify-between gap-3 rounded-lg bg-zinc-900/70 px-3 py-2 text-sm"
                          key={table.table_name}
                        >
                          <span className="text-zinc-300">{table.table_name}</span>
                          <span className="text-zinc-500">
                            {table.row_count.toLocaleString()}
                          </span>
                        </div>
                      ))}
                    </div>
                  </div>
                </div>
              ) : (
                <p className="py-8 text-center text-sm text-zinc-500">
                  {isDatabaseDiagnosticsLoading
                    ? 'Checking database...'
                    : 'No diagnostics loaded.'}
                </p>
              )}
            </div>
          </section>
        </div>
      ) : null}

      {isCommandPaletteOpen ? (
        <div
          className="fixed inset-0 z-50 bg-black/70 px-4 py-[12vh]"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) {
              closeCommandPalette()
            }
          }}
        >
          <section
            className="mx-auto w-full max-w-2xl overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-950 shadow-[0_24px_80px_rgba(0,0,0,0.55)]"
            role="dialog"
            aria-modal="true"
            aria-label="Command palette"
          >
            <div className="flex min-h-14 items-center gap-3 border-b border-zinc-800 px-4">
              <SearchIcon className="h-5 w-5 flex-none text-zinc-500" />
              <label className="sr-only" htmlFor="command-palette-input">
                Search commands
              </label>
              <input
                ref={commandInputRef}
                className="h-14 min-w-0 flex-1 border-0 bg-transparent text-base text-zinc-100 outline-none placeholder:text-zinc-600"
                id="command-palette-input"
                type="search"
                value={commandQuery}
                placeholder="Search commands"
                role="combobox"
                aria-controls="command-palette-results"
                aria-expanded="true"
                aria-activedescendant={
                  activeVisibleCommandIndex >= 0
                    ? `command-${visibleCommandEntries[activeVisibleCommandIndex].id}`
                    : undefined
                }
                onChange={(event) => {
                  setCommandQuery(event.target.value)
                  setActiveCommandIndex(0)
                }}
                onKeyDown={handleCommandPaletteKeyDown}
              />
              <button
                className="grid h-8 w-8 flex-none place-items-center rounded-lg border-0 bg-transparent text-zinc-500 transition-colors hover:bg-zinc-900 hover:text-zinc-100"
                type="button"
                aria-label="Close command palette"
                onClick={closeCommandPalette}
              >
                <XIcon />
              </button>
            </div>

            <div
              className="max-h-[min(28rem,58vh)] overflow-y-auto p-2"
              id="command-palette-results"
              role="listbox"
            >
              {visibleCommandEntries.length > 0 ? (
                visibleCommandEntries.map((command, index) => {
                  const isActive = index === activeVisibleCommandIndex
                  const isDisabled = command.disabledReason !== undefined

                  return (
                    <button
                      className={`flex min-h-16 w-full min-w-0 items-center justify-between gap-4 rounded-xl border-0 px-3 py-2 text-left transition-colors ${
                        isActive
                          ? 'bg-zinc-800/90'
                          : 'bg-transparent hover:bg-zinc-900'
                      } ${isDisabled ? 'text-zinc-500' : 'text-zinc-100'}`}
                      id={`command-${command.id}`}
                      key={command.id}
                      type="button"
                      role="option"
                      aria-selected={isActive}
                      aria-disabled={isDisabled}
                      onMouseEnter={() => setActiveCommandIndex(index)}
                      onClick={() => runCommand(command)}
                    >
                      <span className="min-w-0">
                        <span className="block truncate text-sm font-medium">
                          {command.title}
                        </span>
                        {command.description ? (
                          <span className="mt-0.5 block truncate text-xs text-zinc-500">
                            {command.description}
                          </span>
                        ) : null}
                        {command.disabledReason ? (
                          <span className="mt-1 block text-xs text-amber-300/80">
                            {command.disabledReason}
                          </span>
                        ) : null}
                      </span>
                      <span className="flex-none rounded-lg border border-zinc-800 px-2 py-1 text-[11px] font-medium text-zinc-500">
                        {command.category}
                      </span>
                    </button>
                  )
                })
              ) : (
                <p className="px-3 py-8 text-center text-sm text-zinc-500">
                  No commands found.
                </p>
              )}
            </div>
          </section>
        </div>
      ) : null}
    </>
  )
}

export default App
