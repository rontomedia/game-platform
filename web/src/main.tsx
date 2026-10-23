import '@fontsource-variable/inter'
import './index.css'

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import App from './App.tsx'
import initEngine from './wasm/engine.js'

// The rules engine (Rust, crates/engine-wasm) must be loaded before
// anything asks it a question.
await initEngine()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
