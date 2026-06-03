import { invoke } from '@tauri-apps/api/core'
import { type FormEvent, useEffect, useState } from 'react'

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

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

const isTauriRuntime = () => window.__TAURI_INTERNALS__ !== undefined

function titleFromMessage(content: string) {
  const title = content.trim()
  return title.length > 48 ? `${title.slice(0, 45)}...` : title
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

function App() {
  const [isSidebarOpen, setIsSidebarOpen] = useState(true)
  const [chats, setChats] = useState<ChatSummary[]>([])
  const [activeChatId, setActiveChatId] = useState<string | null>(null)
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [draft, setDraft] = useState('')
  const [historyError, setHistoryError] = useState<string | null>(null)

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

  async function refreshChats() {
    if (!isTauriRuntime()) {
      return
    }

    const loadedChats = await invoke<ChatSummary[]>('list_chats')
    setChats(loadedChats)
  }

  async function handleNewChat() {
    setActiveChatId(null)
    setMessages([])
    setDraft('')
  }

  async function handleSelectChat(chatId: string) {
    setActiveChatId(chatId)
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()

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
      return
    }

    try {
      let chatId = activeChatId

      if (!chatId) {
        const chat = await invoke<ChatSummary>('create_chat', {
          title: titleFromMessage(content),
        })
        chatId = chat.id
        setActiveChatId(chat.id)
      }

      await invoke<ChatMessage>('add_message', {
        chatId,
        role: 'user',
        content,
      })

      const [loadedMessages] = await Promise.all([
        invoke<ChatMessage[]>('get_messages', { chatId }),
        refreshChats(),
      ])

      setMessages(loadedMessages)
      setDraft('')
      setHistoryError(null)
    } catch (error) {
      setHistoryError(String(error))
    }
  }

  return (
    <div className="flex min-h-svh w-full flex-col bg-black text-zinc-50 md:flex-row">
      <aside
        className={`w-full border-b border-zinc-800/80 px-5 py-5 transition-[width] duration-200 md:flex-none md:border-r md:border-b-0 md:px-4 md:py-4 ${
          isSidebarOpen ? 'md:w-64' : 'md:w-18'
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
          <nav className="grid gap-2">
            <button
              className="flex min-h-11 w-full items-center gap-3 rounded-xl border-0 bg-zinc-900 px-3 text-left text-base leading-none text-zinc-50 transition-colors hover:bg-zinc-800"
              type="button"
              onClick={handleNewChat}
            >
              <NewChatIcon className="h-5.5 w-5.5 flex-none" />
              <span>New chat</span>
            </button>

            <button
              className="flex min-h-11 w-full items-center gap-3 rounded-xl border-0 bg-transparent px-3 text-left text-base leading-none text-zinc-50 transition-colors hover:bg-zinc-900"
              type="button"
            >
              <SearchIcon className="h-5.5 w-5.5 flex-none" />
              <span>Search chats</span>
            </button>

            <div className="mt-4 grid gap-1 border-t border-zinc-800/80 pt-4">
              {chats.length > 0 ? (
                chats.map((chat) => (
                  <button
                    className={`min-h-10 rounded-lg border-0 px-3 text-left text-sm transition-colors ${
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
                  No saved chats yet
                </p>
              )}
            </div>
          </nav>
        ) : null}
      </aside>

      <main
        className="relative min-h-[50svh] flex-1 overflow-hidden md:min-h-svh"
        aria-label="Chat"
      >
        <div className="absolute top-4 left-4">
          <label className="sr-only" htmlFor="model-select">
            Model
          </label>
          <select
            className="h-10 rounded-xl border border-zinc-800 bg-black px-3 pr-9 text-sm font-medium text-zinc-200 outline-none transition-colors hover:bg-zinc-950 focus:border-zinc-600"
            id="model-select"
            defaultValue="placeholder"
          >
            <option value="placeholder">Select model</option>
          </select>
        </div>

        <div className="absolute inset-x-0 top-20 bottom-28 overflow-y-auto px-4">
          <div className="mx-auto flex max-w-3xl flex-col gap-4">
            {messages.map((message) => (
              <article
                className={`max-w-[78%] rounded-2xl px-4 py-3 text-sm leading-6 ${
                  message.role === 'user'
                    ? 'ml-auto bg-zinc-100 text-zinc-950'
                    : 'mr-auto bg-zinc-900 text-zinc-100'
                }`}
                key={message.id}
              >
                {message.content}
              </article>
            ))}

            {historyError ? (
              <p className="rounded-xl border border-red-900/60 bg-red-950/30 px-4 py-3 text-sm text-red-200">
                {historyError}
              </p>
            ) : null}
          </div>
        </div>

        <form
          className="absolute bottom-4 left-1/2 flex min-h-14 w-[calc(100%-2rem)] max-w-[760px] -translate-x-1/2 items-center rounded-[1.75rem] border border-zinc-700/70 bg-zinc-900 px-4 shadow-[0_12px_40px_rgba(0,0,0,0.35)] md:bottom-6 md:min-h-[58px] md:px-[18px]"
          onSubmit={handleSubmit}
        >
          <label className="sr-only" htmlFor="chat-input">
            Message
          </label>
          <input
            className="w-full min-w-0 border-0 bg-transparent text-lg leading-tight text-zinc-100 outline-none placeholder:text-zinc-400"
            id="chat-input"
            type="text"
            placeholder="Ask anything"
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
          />
        </form>
      </main>
    </div>
  )
}

export default App
