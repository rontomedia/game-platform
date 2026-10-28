import { describe, expect, it } from 'vitest'

import { LANGUAGES, pickLanguage, resolveLanguage } from './languages.ts'

describe('pickLanguage', () => {
  it('takes the first preferred language the game has', () => {
    expect(pickLanguage(['ja', 'de-AT', 'fr'])).toBe('de')
    expect(pickLanguage(['pl-PL', 'en'])).toBe('pl')
  })

  it('matches a region exactly, or by language alone', () => {
    expect(pickLanguage(['pt-BR'])).toBe('pt-BR')
    expect(pickLanguage(['pt-PT'])).toBe('pt-BR')
    expect(pickLanguage(['pt'])).toBe('pt-BR')
    expect(pickLanguage(['en-GB'])).toBe('en')
    expect(pickLanguage(['SV-se'])).toBe('sv')
  })

  it('falls back to English', () => {
    expect(pickLanguage(['ja', 'zh-CN'])).toBe('en')
    expect(pickLanguage([])).toBe('en')
  })
})

describe('resolveLanguage', () => {
  it('lets a forced language win, then the choice, then the browser', () => {
    expect(resolveLanguage('pl', ['de'], 'el')).toBe('el')
    expect(resolveLanguage('pl', ['de'], 'xx')).toBe('pl')
    expect(resolveLanguage('auto', ['de'], null)).toBe('de')
  })
})

type Catalogue = { [key: string]: string | Catalogue }

const catalogues = Object.fromEntries(
  Object.entries(import.meta.glob<Catalogue>('./locales/*.json', { eager: true, import: 'default' })).map(
    ([path, catalogue]) => [path.replace(/^.*\/(.*)\.json$/, '$1'), catalogue],
  ),
)

/** Every text in a catalogue, by its dotted key. */
function flatten(catalogue: Catalogue, prefix = ''): Map<string, string> {
  const texts = new Map<string, string>()
  for (const [key, value] of Object.entries(catalogue)) {
    if (typeof value === 'string') texts.set(prefix + key, value)
    else for (const [inner, text] of flatten(value, `${prefix}${key}.`)) texts.set(inner, text)
  }
  return texts
}

const PLURAL = /_(zero|one|two|few|many|other)$/

/** The texts grouped by key, a plural's forms under one key. */
function messages(catalogue: Catalogue): Map<string, Map<string, string>> {
  const grouped = new Map<string, Map<string, string>>()
  for (const [key, text] of flatten(catalogue)) {
    const form = PLURAL.exec(key)?.[1] ?? ''
    const base = form ? key.slice(0, -form.length - 1) : key
    if (!grouped.has(base)) grouped.set(base, new Map())
    grouped.get(base)!.set(form, text)
  }
  return grouped
}

const variables = (texts: Iterable<string>) =>
  [...new Set([...texts].flatMap((text) => [...text.matchAll(/\{\{\s*(\w+)/g)].map((m) => m[1])))].sort()

describe('catalogues', () => {
  const english = messages(catalogues.en)

  it('are only for languages the game lists', () => {
    for (const language of Object.keys(catalogues)) expect(Object.keys(LANGUAGES)).toContain(language)
  })

  for (const [language, catalogue] of Object.entries(catalogues)) {
    describe(language, () => {
      const own = messages(catalogue)

      it('has the same keys as English', () => {
        expect([...own.keys()].sort()).toEqual([...english.keys()].sort())
      })

      it("has each plural in the language's own forms", () => {
        const forms = [...new Intl.PluralRules(language).resolvedOptions().pluralCategories].sort()
        for (const [key, texts] of own) {
          const plural = [...english.get(key)?.keys() ?? []].some(Boolean)
          expect([...texts.keys()].sort(), key).toEqual(plural ? forms : [''])
        }
      })

      it('uses the same values as English', () => {
        for (const [key, texts] of own) {
          expect(variables(texts.values()), key).toEqual(variables(english.get(key)?.values() ?? []))
        }
      })
    })
  }
})
