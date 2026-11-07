import react from '@vitejs/plugin-react'
import { defineConfig } from 'vitest/config'

// Tauri expects a fixed dev port (see devUrl in src-tauri/tauri.conf.json).
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 5174, strictPort: true },
  test: { environment: 'jsdom' },
})
