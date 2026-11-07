// What a new user sees before the first send: examples that work right away.
const EXAMPLES = [
  { method: 'GET', url: 'api.github.com/repos/tauri-apps/tauri', what: 'A public JSON API' },
  { method: 'GET', url: 'api.github.com/zen', what: 'Plain text' },
  { method: 'GET', url: 'localhost:8080', what: 'Something on your own machine' },
]

export function StartHint({ onPick }: { onPick: (method: string, url: string) => void }) {
  return (
    <div className="start">
      <p>Type an address and press <b>Send</b>. You can leave out http:// and https://, Crane picks the right one.</p>
      <div className="lbl">Try one</div>
      <ul>
        {EXAMPLES.map((e) => (
          <li key={e.url}>
            <button onClick={() => onPick(e.method, e.url)}>
              <span className={`m m-${e.method}`}>{e.method}</span>
              <span className="mono">{e.url}</span>
              <span className="quiet push">{e.what}</span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  )
}
