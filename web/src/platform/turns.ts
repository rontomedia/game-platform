import type { TFunction } from 'i18next'

/** The turn lengths a room may have (the server's `TURN_SECONDS`). */
export const TURN_SECONDS = [30, 60, 90, 120, 180]

/** A turn's length, short: in minutes when it's whole ones. */
export function turnLength(t: TFunction, seconds: number): string {
  return seconds % 60 === 0 ? t('units.minutes', { n: seconds / 60 }) : t('units.seconds', { n: seconds })
}
