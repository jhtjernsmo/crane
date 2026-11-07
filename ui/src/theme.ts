// Light, dark or whatever the system uses. The choice is remembered per machine.
export type Theme = 'system' | 'light' | 'dark'

const KEY = 'crane.theme'
export const THEMES: Theme[] = ['system', 'light', 'dark']

export function loadTheme(): Theme {
  try {
    const saved = localStorage.getItem(KEY)
    return THEMES.includes(saved as Theme) ? (saved as Theme) : 'system'
  } catch {
    return 'system'
  }
}

export function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme
  try {
    localStorage.setItem(KEY, theme)
  } catch {
    // Private windows can refuse storage; the theme still applies for now.
  }
}
