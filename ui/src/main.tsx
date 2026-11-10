import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import '@fontsource/ibm-plex-sans/400.css'
import '@fontsource/ibm-plex-sans/500.css'
import '@fontsource/ibm-plex-sans/600.css'
import '@fontsource/ibm-plex-mono/400.css'
import '@fontsource/ibm-plex-mono/500.css'
import '@fontsource/ibm-plex-mono/600.css'
import './styles.css'
import App from './App'

// No browser menu on right-click (Back, Reload, Inspect...). Text fields keep
// theirs for cut, copy and paste; Crane's own menus call preventDefault first.
document.addEventListener('contextmenu', (e) => {
  const el = e.target as HTMLElement | null
  if (!el?.closest('input, textarea, [contenteditable="true"]')) e.preventDefault()
})

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
