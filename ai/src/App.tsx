type IconProps = {
  className?: string
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

function PlusIcon({ className = 'h-5.5 w-5.5' }: IconProps) {
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
      <path d="M12 5v14" />
      <path d="M5 12h14" />
    </svg>
  )
}

function App() {
  return (
    <div className="flex min-h-svh w-full flex-col bg-black text-zinc-50 md:flex-row">
      <aside
        className="w-full border-b border-zinc-800/80 px-5 py-5 md:w-64 md:flex-none md:border-r md:border-b-0 md:px-4 md:py-4"
        aria-label="Chat navigation"
      >
        <div className="mb-7 flex items-center justify-between gap-4 md:mb-10">
          <div className="flex min-w-0 items-center gap-2.5">
            <ShipWheelLogo className="h-7 w-7 flex-none text-zinc-100" />
            <h1 className="truncate text-[22px] leading-none font-semibold tracking-[-0.01em]">
              Atlas
            </h1>
          </div>

          <button
            className="grid h-9 w-9 place-items-center rounded-xl border-0 bg-transparent p-0 text-zinc-400 transition-colors hover:bg-zinc-900 hover:text-zinc-100"
            type="button"
            aria-label="Toggle sidebar"
          >
            <SidebarToggleIcon />
          </button>
        </div>

        <nav className="grid gap-2">
          <button
            className="flex min-h-11 w-full items-center gap-3 rounded-xl border-0 bg-zinc-900 px-3 text-left text-base leading-none text-zinc-50 transition-colors hover:bg-zinc-800"
            type="button"
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
        </nav>
      </aside>

      <main
        className="relative min-h-[50svh] flex-1 overflow-hidden md:min-h-svh"
        aria-label="Chat"
      >
        <div className="pointer-events-none absolute inset-0 bg-[linear-gradient(rgba(255,255,255,0.035)_1px,transparent_1px),linear-gradient(90deg,rgba(255,255,255,0.025)_1px,transparent_1px)] bg-[size:72px_72px] opacity-35" />
        <ShipWheelLogo className="pointer-events-none absolute top-1/2 left-1/2 h-48 w-48 -translate-x-1/2 -translate-y-1/2 text-zinc-900/50 md:h-64 md:w-64" />

        <form
          className="absolute bottom-4 left-1/2 flex min-h-14 w-[calc(100%-2rem)] max-w-[760px] -translate-x-1/2 items-center rounded-[1.75rem] border border-zinc-700/70 bg-zinc-900 px-4 shadow-[0_12px_40px_rgba(0,0,0,0.35)] md:bottom-6 md:min-h-[58px] md:px-[18px]"
          onSubmit={(event) => event.preventDefault()}
        >
          <button
            className="grid h-8 w-8 flex-none place-items-center rounded-full border-0 bg-transparent p-0 text-zinc-100 transition-colors hover:bg-zinc-800"
            type="button"
            aria-label="Add attachment"
          >
            <PlusIcon />
          </button>

          <label className="sr-only" htmlFor="chat-input">
            Message
          </label>
          <input
            className="ml-3 w-full min-w-0 border-0 bg-transparent text-lg leading-tight text-zinc-100 outline-none placeholder:text-zinc-400"
            id="chat-input"
            type="text"
            placeholder="Ask anything"
          />
        </form>
      </main>
    </div>
  )
}

export default App
