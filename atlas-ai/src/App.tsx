import { invoke } from '@tauri-apps/api/core'
import { type FormEvent, useEffect, useRef, useState } from 'react'
import {
  deleteOllamaModel,
  downloadOllamaModel,
  getOllamaStatus,
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

type ChatMessage = {
  id: number
  chat_id: string
  role: 'user' | 'assistant' | 'system'
  content: string
  created_at: number
}

type UiError = {
  message: string
  details?: string
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
  const [isModelPanelOpen, setIsModelPanelOpen] = useState(false)
  const [isResponding, setIsResponding] = useState(false)
  const [respondingChatId, setRespondingChatId] = useState<string | null>(null)
  const [deletingChatId, setDeletingChatId] = useState<string | null>(null)
  const [isChatSearchOpen, setIsChatSearchOpen] = useState(false)
  const [chatSearchQuery, setChatSearchQuery] = useState('')
  const [chatSearchResults, setChatSearchResults] = useState<ChatSummary[]>([])
  const [isChatSearchLoading, setIsChatSearchLoading] = useState(false)
  const [chatSearchError, setChatSearchError] = useState<string | null>(null)
  const activeChatIdRef = useRef<string | null>(null)

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
      const models = await downloadOllamaModel(model)
      applyOllamaStatus(buildOllamaStatusFromModels(models, model), model)
      setModelError(null)
    } catch (error) {
      setModelError({
        message: 'Could not download model.',
        details: String(error),
      })
    } finally {
      setModelAction(null)
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

  function handleToggleChatSearch() {
    if (isChatSearchOpen) {
      closeChatSearch()
      return
    }

    setIsChatSearchOpen(true)
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
    closeChatSearch()
  }

  async function handleSelectChat(chatId: string) {
    setActiveChatId(chatId)
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
  const modelDownloadsDisabled =
    modelAction !== null ||
    isOllamaStatusLoading ||
    !isTauriRuntime() ||
    ollamaStatus?.status === 'unavailable'

  return (
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
          <button
            className="absolute top-4 right-4 z-10 grid h-10 w-10 place-items-center rounded-xl border border-red-950/80 bg-black/90 text-red-300 shadow-[0_12px_36px_rgba(0,0,0,0.35)] transition-colors hover:bg-red-950/30 hover:text-red-100 disabled:cursor-not-allowed disabled:opacity-50"
            type="button"
            aria-label="Delete chat"
            disabled={deletingChatId === activeChatId}
            onClick={handleDeleteActiveChat}
          >
            <TrashIcon />
          </button>
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
                {message.content}
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
  )
}

export default App
