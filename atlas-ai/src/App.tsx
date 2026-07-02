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
  deleteOllamaModel,
  downloadOllamaModel,
  exportChat,
  getOllamaStatus,
  listJobs,
  type ChatExport,
  type ChatExportFormat,
  type ChatMessage,
  type GenerationRun,
  type Job,
  type JobEvent,
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

type CommandId =
  | 'chat.new'
  | 'chat.search'
  | 'chat.delete_active'
  | 'model.manager.open'
  | 'model.refresh'
  | 'model.switch.open'
  | `model.switch:${string}`
  | `model.download:${string}`
  | `chat.export.${ChatExportFormat}`
  | 'settings.open'
  | 'diagnostics.open'
  | 'model_lab.open'
  | 'knowledge.index_folder'

type AppCommand = {
  id: CommandId
  title: string
  category: 'Chat' | 'Model' | 'App' | 'Knowledge'
  description?: string
  disabledReason?: string
  keywords?: string[]
  run: () => void | Promise<void>
}

type CommandRegistryContext = {
  activeChatId: string | null
  deletingChatId: string | null
  exportAction: ChatExportFormat | null
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

function MessageDiagnostics({ run }: { run: GenerationRun | null }) {
  if (!run) {
    return null
  }

  const metrics = [
    ['Model', run.model_name],
    ['Status', generationStatusLabel(run.status)],
    ['Total', formatDurationMs(run.total_duration_ms)],
    ['First token', formatDurationMs(getTimeToFirstToken(run))],
    ['Prompt', formatCount(run.prompt_eval_count)],
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

  const currentMb = job.progress_current / 1024 / 1024
  const totalMb = job.progress_total / 1024 / 1024
  return `${currentMb.toFixed(1)} / ${totalMb.toFixed(1)} MB`
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

  commands.push(
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
      description: 'Review local app health.',
      disabledReason: 'Diagnostics Center lands in Chunk 15.',
      keywords: ['health status'],
      run: () => undefined,
    },
    {
      id: 'model_lab.open',
      title: 'Open Model Lab',
      category: 'Model',
      description: 'Compare local model behavior.',
      disabledReason: 'Model Lab lands in Chunk 9.',
      keywords: ['benchmark evaluate'],
      run: () => undefined,
    },
    {
      id: 'knowledge.index_folder',
      title: 'Index folder',
      category: 'Knowledge',
      description: 'Add local files to the knowledge workspace.',
      disabledReason: 'Knowledge workspace lands in Chunk 12.',
      keywords: ['rag documents files'],
      run: () => undefined,
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

function App() {
  const [isSidebarOpen, setIsSidebarOpen] = useState(true)
  const [chats, setChats] = useState<ChatSummary[]>([])
  const [activeChatId, setActiveChatId] = useState<string | null>(null)
  const [messages, setMessages] = useState<ChatMessage[]>([])
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
  const [jobs, setJobs] = useState<Job[]>([])
  const [isModelPanelOpen, setIsModelPanelOpen] = useState(false)
  const [isResponding, setIsResponding] = useState(false)
  const [respondingChatId, setRespondingChatId] = useState<string | null>(null)
  const [deletingChatId, setDeletingChatId] = useState<string | null>(null)
  const [exportAction, setExportAction] = useState<ChatExportFormat | null>(null)
  const [isExportMenuOpen, setIsExportMenuOpen] = useState(false)
  const [isChatSearchOpen, setIsChatSearchOpen] = useState(false)
  const [chatSearchQuery, setChatSearchQuery] = useState('')
  const [chatSearchResults, setChatSearchResults] = useState<ChatSummary[]>([])
  const [isChatSearchLoading, setIsChatSearchLoading] = useState(false)
  const [chatSearchError, setChatSearchError] = useState<string | null>(null)
  const [isCommandPaletteOpen, setIsCommandPaletteOpen] = useState(false)
  const [commandQuery, setCommandQuery] = useState('')
  const [activeCommandIndex, setActiveCommandIndex] = useState(0)
  const activeChatIdRef = useRef<string | null>(null)
  const commandInputRef = useRef<HTMLInputElement | null>(null)

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

      invoke<ChatSummary[]>('search_chats', { query })
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

  async function handleNewChat() {
    setActiveChatId(null)
    setMessages([])
    setDraft('')
    setIsExportMenuOpen(false)
    closeChatSearch()
  }

  function openModelManager() {
    setIsModelPanelOpen(true)
  }

  async function handleSelectChat(chatId: string) {
    setActiveChatId(chatId)
    setIsExportMenuOpen(false)
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
  const visibleChats = hasChatSearch
    ? isTauriRuntime()
      ? chatSearchResults
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

              {visibleChats.length > 0 ? (
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

            {messages.map((message) => (
              <article
                className={`max-w-[78%] overflow-hidden rounded-2xl px-4 py-3 text-sm leading-6 break-words ${
                  message.role === 'user'
                    ? 'ml-auto bg-zinc-100 text-zinc-950'
                    : 'mr-auto bg-zinc-900 text-zinc-100'
                }`}
                key={message.id}
              >
                <div>{message.content}</div>
                {message.role === 'assistant' ? (
                  <MessageDiagnostics run={message.generation_run} />
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
