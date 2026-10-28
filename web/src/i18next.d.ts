import 'i18next'

import type en from './locales/en.json'

// Keys and interpolated values checked against the English catalogue.
declare module 'i18next' {
  interface CustomTypeOptions {
    defaultNS: 'translation'
    resources: { translation: typeof en }
  }
}
