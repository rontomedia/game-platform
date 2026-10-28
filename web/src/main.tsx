import '@fontsource-variable/inter'
import './index.css'

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import App from './App.tsx'
import { initI18n } from './i18n.ts'
import { savedSettings } from './settings.ts'
import initEngine from './wasm/engine.js'

// The rules engine (Rust, crates/engine-wasm) must be loaded before
// anything asks it a question, and the player's language before anything
// is shown.
await Promise.all([initEngine(), initI18n(savedSettings().language)])

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
