import '@fontsource-variable/inter'
// Scripts Inter lacks; each only downloads when a page shows its letters.
import '@fontsource-variable/noto-sans-hebrew'
import '@fontsource-variable/noto-sans-arabic'
import './index.css'

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import App from './App.tsx'
import { initI18n } from './platform/i18n.ts'
import { savedSettings } from './platform/settings.ts'

// The player's language before anything is shown. (Each game loads its
// own rules engine, WASM, with its page: see App.tsx.)
await initI18n(savedSettings().language)

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
