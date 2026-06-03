import './App.css'

function App() {
  return (
    <div className="chat-shell">
      <aside className="sidebar" aria-label="Chat navigation">
        <div className="sidebar-header">
          <h1>Atlas</h1>
          <button className="icon-button" type="button" aria-label="Toggle sidebar">
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <rect x="3" y="4" width="18" height="16" rx="4" />
              <path d="M9 4v16" />
            </svg>
          </button>
        </div>

        <nav className="sidebar-nav">
          <button className="nav-item" type="button">
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
              <path d="M18.4 2.6a2.1 2.1 0 0 1 3 3L12 15l-4 1 1-4Z" />
            </svg>
            <span>New chat</span>
          </button>

          <button className="nav-item" type="button">
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <circle cx="11" cy="11" r="7" />
              <path d="m20 20-4.6-4.6" />
            </svg>
            <span>Search chats</span>
          </button>
        </nav>
      </aside>

      <main className="chat-main" aria-label="Chat">
        <form className="composer" onSubmit={(event) => event.preventDefault()}>
          <button className="composer-action" type="button" aria-label="Add attachment">
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M12 5v14" />
              <path d="M5 12h14" />
            </svg>
          </button>
          <label className="visually-hidden" htmlFor="chat-input">
            Message
          </label>
          <input id="chat-input" type="text" placeholder="Ask anything" />
        </form>
      </main>
    </div>
  )
}

export default App
