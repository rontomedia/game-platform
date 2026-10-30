import { useTranslation } from 'react-i18next'

import type { RoomInfo } from './protocol.ts'
import { Button } from './ui.tsx'

/**
 * In place of the tray for someone watching: what's going on and, for the
 * host of a game only bots play, buttons to pause it and step through it.
 */
export function WatchBar({
  room,
  over,
  note,
  noteColor,
  send,
}: {
  room: RoomInfo<unknown, unknown>
  over: boolean
  note: string
  noteColor: string
  send: (message: { type: 'step' } | { type: 'pause'; paused: boolean }) => void
}) {
  const { t } = useTranslation()
  const host = room.watching !== undefined && room.watchers[room.watching]?.host
  const botsOnly = room.seats.every((seat) => seat.bot !== undefined)
  return (
    <section className="tray flex flex-wrap items-center gap-x-4 gap-y-2 rounded-3xl p-3 sm:p-4 short:rounded-2xl short:p-2">
      <span className="rounded-full bg-violet-400/15 px-2.5 py-1 text-xs font-semibold uppercase tracking-wide text-violet-200">
        {t('watch.badge')}
      </span>
      <p className={`min-w-0 flex-1 text-sm font-medium short:truncate ${noteColor}`}>{note}</p>
      {host && botsOnly && !over && (
        <div className="flex gap-2">
          <Button look="quiet" disabled={!room.paused} onClick={() => send({ type: 'step' })} title={t('watch.stepHint')}>
            {t('watch.step')}
          </Button>
          <Button look="primary" className="min-w-24" onClick={() => send({ type: 'pause', paused: !room.paused })}>
            {room.paused ? t('watch.resume') : t('watch.pause')}
          </Button>
        </div>
      )}
    </section>
  )
}
