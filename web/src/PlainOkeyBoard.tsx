import {
  DndContext,
  DragOverlay,
  KeyboardSensor,
  MouseSensor,
  TouchSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from '@dnd-kit/core'
import { arrayMove, sortableKeyboardCoordinates } from '@dnd-kit/sortable'
import { AnimatePresence } from 'motion/react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { compareTiles, type RackSort } from './arrange.ts'
import { TopBar, WatchBar, type Clock } from './Board.tsx'
import { noticeText, problemText, type Problem } from './messages.ts'
import { DiscardPile, DiscardSpot, Middle, NextSpot, Overlay, Plans, Rack } from './OkeyBoard.tsx'
import {
  collisions,
  DISCARD,
  FalseOkeyMark,
  FINISH,
  isPlanTarget,
  joinNew,
  NEW_PLAN,
  NEXT,
  OFFER,
  plainSet,
  RACK,
  rackAndPlans,
  sameOptional,
  sameTile,
  type SavedRack,
  TAKEN,
  tilesOf,
  type Placed,
} from './okeyParts.ts'
import type { Settings } from './settings.ts'
import { remember, remembered } from './storage.ts'
import { TileFace } from './TileView.tsx'
import { Button } from './ui.tsx'
import type { PlainOkeyGame, Status } from './useGame.ts'
import { useNow } from './useNow.ts'
import { okeyComplete, type ClientMessage, type Notice, type RoomInfo } from './wasm/engine.js'

/**
 * Plain Okey's board: no table. The indicator, the okey and the pool, and
 * everyone's discards; the rack, a spot to discard on, and in the middle a
 * spot to finish on: dropping the 15th tile there shows the other 14 as a
 * winning hand (all in sets, or seven pairs). Draw (the button, or the
 * pool), or drag the previous player's discard in. Plans above the rack
 * group its tiles into the sets of a winning hand, any time.
 */

/** Remembered for the tab: the player's rack order and plans in their current room. */
const RACK_KEY = 'rummy.plainOkeyRack'

/** Give it `key={game.version}`: a new state starts the turn afresh. */
export function PlainOkeyBoard({
  game,
  room,
  status,
  problem,
  notice,
  settings,
  onSettings,
  send,
  leave,
}: {
  game: PlainOkeyGame
  room: RoomInfo
  status: Status
  problem: Problem | null
  notice: Notice | null
  settings: Settings
  onSettings: (change: Partial<Settings>) => void
  send: (message: ClientMessage) => void
  leave: () => void
}) {
  const { t } = useTranslation()
  const { view } = game
  const you = view.you
  const watching = you === undefined
  const over = view.winners !== undefined
  const myTurn = !watching && view.current === you && !view.end && !room.paused
  const players = view.rackSizes.length
  const previous = (view.current + players - 1) % players
  const offer = view.discards[previous]?.at(-1)
  const canTake = myTurn && view.phase === 'draw' && offer !== undefined

  const [sort, setSort] = useState<RackSort>(() => remembered<RackSort>('rummy.rackSort') ?? 'custom')
  // Plans and the tile set aside belong to one hand.
  const handKey = `${view.points.join()}:${JSON.stringify(view.indicator)}`
  const [tiles, setTiles] = useState(() => rackAndPlans(RACK_KEY, room.code, handKey, view.rack, sort))
  const { rack, plans, parked } = tiles
  /** Sets the board's tiles, telling the server when the tile set aside to discard next changes. */
  const update = (next: typeof tiles) => {
    if (!watching && !sameOptional(next.parked?.tile, parked?.tile)) send({ type: 'discardNext', tile: next.parked?.tile })
    setTiles(next)
  }
  const [taken, setTaken] = useState(false)
  const [refused, setRefused] = useState<Problem | null>(null)
  const [dragging, setDragging] = useState<Placed | null>(null)
  /** The plan last changed, which a double-clicked tile joins if it fits. */
  const lastPlan = useRef<number | null>(null)

  useEffect(() => {
    const saved: SavedRack = {
      code: room.code,
      tiles: tilesOf(rack.filter((p) => p.id !== TAKEN)),
      hand: handKey,
      plans: plans.map(tilesOf),
      next: parked?.tile,
    }
    remember(RACK_KEY, saved, 'session')
  }, [rack, plans, parked, room.code, handKey])

  // Every tile the player holds, planned, set aside or not.
  const all = [...rack, ...plans.flat(), ...(parked ? [parked] : [])]
  const canDiscard = myTurn && (view.phase === 'play' || taken)
  // Whether some tile, discarded, leaves a winning 14.
  const canFinish =
    canDiscard && all.some((placed) => okeyComplete(view.okey, tilesOf(all.filter((p) => p.id !== placed.id))) !== undefined)
  // The tile this player last drew, marked wherever they keep it.
  const drawnId = game.drawn && all.findLast((p) => sameTile(p.tile, game.drawn!))?.id

  /** The rack, plans and tile set aside without tile `id` (a plan left empty goes). */
  const without = (id: string) => ({
    rack: rack.filter((p) => p.id !== id),
    plans: plans.map((plan) => plan.filter((p) => p.id !== id)).filter((plan) => plan.length > 0),
    parked: parked?.id === id ? undefined : parked,
  })

  const sensors = useSensors(
    useSensor(MouseSensor, { activationConstraint: { distance: 4 } }),
    // A short hold, so a swipe still scrolls.
    useSensor(TouchSensor, { activationConstraint: { delay: 150, tolerance: 8 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  )

  const end = (placed: Placed, finish: boolean) => {
    const rest = all.filter((p) => p.id !== placed.id)
    if (finish && okeyComplete(view.okey, tilesOf(rest)) === undefined) {
      return setRefused({ kind: 'okey', error: { kind: 'notComplete' } })
    }
    update(without(placed.id))
    send({ type: 'discard', takeDiscard: taken, table: [], discard: placed.tile, finish })
  }

  /**
   * A double-clicked tile, any time: from the rack into the plan last
   * changed if it fits, else a new plan; a planned tile, or the one set
   * aside, back to the rack.
   * Nothing is discarded or finished by double-click.
   */
  const quickMove = (placed: Placed) => {
    if (watching || view.end || placed.id === TAKEN) return
    const left = without(placed.id)
    if (!rack.includes(placed)) return update({ ...left, rack: [...left.rack, placed] })
    const p = lastPlan.current
    // A 1 after 13 ends a run here (12 13 1), which the classic placing doesn't know.
    const wraps = p !== null && plans[p] && plainSet(view.okey, tilesOf([...plans[p], placed])) ? [...plans[p], placed] : null
    const joined = wraps ?? (p !== null && plans[p] ? joinNew(view.okey, plans[p], placed) : null)
    if (joined) return update({ ...left, plans: plans.map((plan, i) => (i === p ? joined : plan)) })
    lastPlan.current = left.plans.length
    update({ ...left, plans: [...left.plans, [placed]] })
  }

  const onDragStart = ({ active }: DragStartEvent) => {
    setRefused(null)
    const id = String(active.id)
    setDragging(id === OFFER ? (offer ? { id: TAKEN, tile: offer } : null) : (all.find((p) => p.id === id) ?? null))
  }

  const onDragEnd = ({ active, over }: DragEndEvent) => {
    setDragging(null)
    if (!over) return
    const id = String(active.id)
    const to = String(over.id)
    if (id === OFFER) {
      // Taken into the rack.
      if (!offer || !canTake || taken || !(to === RACK || rack.some((p) => p.id === to))) return
      const at = to === RACK ? rack.length : rack.findIndex((p) => p.id === to)
      const next = [...rack]
      next.splice(at, 0, { id: TAKEN, tile: offer })
      update({ ...tiles, rack: next })
      setTaken(true)
      return
    }
    const placed = all.find((p) => p.id === id)
    if (!placed) return
    if (to === OFFER && id === TAKEN) {
      update(without(TAKEN))
      setTaken(false)
      return
    }
    if ((to === DISCARD || to === FINISH) && canDiscard) return end(placed, to === FINISH)
    // Set aside to discard next, any time; a tile there before goes back to the rack.
    if (to === NEXT) {
      if (id === TAKEN) return
      const left = without(id)
      return update({ ...left, rack: parked && parked.id !== id ? [...left.rack, parked] : left.rack, parked: placed })
    }
    // Into the plans: any of the player's tiles but the discard taken.
    if (to === NEW_PLAN || isPlanTarget(to)) {
      if (id === TAKEN) return
      const left = without(id)
      let p = Number(to.slice('plan-'.length))
      const from = plans.findIndex((plan) => plan.includes(placed))
      if (from >= 0 && from < p && plans[from].length === 1) p--
      if (to === NEW_PLAN || !left.plans[p]) {
        lastPlan.current = left.plans.length
        return update({ ...left, plans: [...left.plans, [placed]] })
      }
      // Pointer left of the plan's middle: its front.
      const translated = active.rect.current.translated
      const front = translated !== null && translated.left + translated.width / 2 < over.rect.left + over.rect.width / 2
      lastPlan.current = p
      return update({ ...left, plans: left.plans.map((plan, i) => (i === p ? (front ? [placed, ...plan] : [...plan, placed]) : plan)) })
    }
    const overRack = to === RACK || rack.some((p) => p.id === to)
    if (!overRack) return
    // Back from a plan or the slot, at the place it's dropped.
    if (!rack.includes(placed)) {
      const left = without(id)
      const at = to === RACK ? left.rack.length : left.rack.findIndex((p) => p.id === to)
      const next = [...left.rack]
      next.splice(at < 0 ? next.length : at, 0, placed)
      return update({ ...left, rack: next })
    }
    const target = to === RACK ? rack.length - 1 : rack.findIndex((p) => p.id === to)
    if (target < 0) return
    setSort('custom')
    update({ ...tiles, rack: arrayMove(rack, rack.indexOf(placed), target) })
  }

  const sortRack = (by: 'number' | 'color') => {
    setSort(by)
    remember('rummy.rackSort', by)
    update({ ...tiles, rack: [...rack].sort((a, b) => compareTiles(a.tile, b.tile, by)) })
  }

  const name = (seat: number) => room.seats[seat]?.name ?? t('board.player', { n: seat + 1 })

  const now = useNow()
  const clock = (seat: number): Clock => {
    const current = seat === view.current && !view.end
    const overdue = game.turnEnds === undefined ? 0 : Math.max(0, now - game.turnEnds)
    const turn = current && game.turnEnds !== undefined ? Math.max(0, Math.ceil((game.turnEnds - now) / 1000)) : undefined
    const bankMs = game.banks?.[seat]
    const bank = bankMs === undefined ? undefined : Math.max(0, Math.ceil((current ? bankMs - overdue : bankMs) / 1000))
    return { turn, bank }
  }

  const note = ((): { tone: 'neutral' | 'good' | 'bad'; text: string } => {
    const trouble = refused ?? problem
    if (trouble) return { tone: 'bad', text: problemText(t, trouble) }
    if (notice) return { tone: 'bad', text: noticeText(t, notice) }
    if (over) return { tone: 'neutral', text: t('board.over') }
    if (view.end) return { tone: 'neutral', text: t('okey.handOver') }
    if (room.paused) return { tone: 'neutral', text: t('board.paused', { name: name(view.current) }) }
    if (!myTurn) {
      const playing = t('board.playing', { name: name(view.current) })
      return { tone: 'neutral', text: watching ? t('board.sentences', { first: t('board.youreWatching'), then: playing }) : playing }
    }
    if (view.phase === 'draw' && !taken) {
      return { tone: 'neutral', text: canTake ? t('okey.plainYourTurn', { name: name(previous) }) : t('okey.yourTurnDraw') }
    }
    if (canFinish) return { tone: 'good', text: t('okey.canFinish') }
    return { tone: 'neutral', text: t('okey.plainPlay') }
  })()
  const noteColor = { neutral: 'text-slate-300', good: 'text-emerald-300', bad: 'text-amber-300' }[note.tone]

  const points = view.points.map((p, seat) => `${name(seat)} ${p}`).join(' · ')

  return (
    <FalseOkeyMark value={room.options.falseOkeyMarks ? view.okey : undefined}>
      <DndContext sensors={sensors} collisionDetection={collisions} onDragStart={onDragStart} onDragEnd={onDragEnd} onDragCancel={() => setDragging(null)}>
        <div className="flex h-dvh flex-col gap-3 p-2 sm:gap-4 sm:p-4 short:gap-2 short:p-2">
          <TopBar
            room={room}
            view={{ rackSizes: view.rackSizes, you, current: view.end ? -1 : view.current, pool: view.pool, outcome: over || undefined }}
            clock={clock}
            status={status}
            settings={settings}
            onSettings={onSettings}
            leave={leave}
          />

          <div className="relative flex min-h-0 flex-1 flex-col">
            <main
              dir="ltr"
              className="felt relative flex min-h-0 flex-1 flex-col gap-3 overflow-auto rounded-3xl p-3 sm:p-5 short:rounded-2xl short:p-2"
            >
              <div className="flex flex-wrap items-start gap-3">
                <Middle
                  indicator={view.indicator}
                  okey={view.okey}
                  pool={view.pool}
                  caption={`${t('okey.points')}: ${points}`}
                  canDraw={myTurn && view.phase === 'draw' && !taken}
                  onDraw={() => send({ type: 'draw' })}
                />
                <div className="flex min-w-0 flex-1 flex-wrap justify-end gap-2">
                  {view.discards.map((pile, seat) => (
                    <DiscardPile
                      key={seat}
                      name={name(seat)}
                      pile={pile}
                      offered={seat === previous && canTake && !taken}
                      returnable={seat === previous && taken}
                    />
                  ))}
                </div>
              </div>
              <div className="grid flex-1 place-items-center">
                <DiscardSpot id={FINISH} label={t('okey.finishHere')} active={canDiscard} className="h-32 w-full max-w-md text-sm" />
              </div>
              <AnimatePresence>
                {view.end && !over && <HandOver game={game} name={name} now={now} />}
                {over && <GameOver game={game} name={name} leave={leave} />}
              </AnimatePresence>
            </main>
          </div>

          {watching ? (
            <WatchBar room={room} over={over} note={note.text} noteColor={noteColor} send={send} />
          ) : (
            <section className={`tray rounded-3xl p-3 sm:p-4 short:rounded-2xl short:p-2 ${myTurn && game.turnSince === game.version ? 'turn-cue' : ''}`}>
              <div className="mb-3 flex flex-wrap items-center gap-x-4 gap-y-2 short:mb-2">
                <p className={`min-w-0 flex-1 text-sm font-medium short:truncate ${noteColor}`}>{note.text}</p>
                <div className="flex gap-1 rounded-xl bg-black/30 p-1 text-xs font-semibold" role="group" aria-label={t('board.sort')}>
                  {(['number', 'color'] as const).map((by) => (
                    <button
                      key={by}
                      onClick={() => sortRack(by)}
                      aria-pressed={sort === by}
                      className={`cursor-pointer rounded-lg px-2 py-1 ${sort === by ? 'bg-white/15 text-slate-100' : 'text-slate-400 hover:text-slate-200'}`}
                    >
                      {by === 'number' ? t('board.sortNumber') : t('board.sortColor')}
                    </button>
                  ))}
                </div>
                <div className={`flex gap-2 ${myTurn ? '' : 'invisible'}`} aria-hidden={!myTurn}>
                  <Button onClick={() => send({ type: 'draw' })} disabled={!myTurn || view.phase !== 'draw' || taken}>
                    {t('board.draw')}
                  </Button>
                </div>
              </div>
              <Plans plans={plans} okey={view.okey} valid={plainSet} canPlay={false} drawnId={drawnId} onQuick={quickMove} />
              <div className="flex items-stretch gap-3">
                <Rack tiles={rack} sortable={!watching} okey={view.okey} drawnId={drawnId} onQuick={quickMove} />
                <NextSpot tile={parked} okey={view.okey} drawnId={drawnId} onQuick={quickMove} />
                <DiscardSpot id={DISCARD} label={t('okey.discardHere')} active={canDiscard} />
              </div>
            </section>
          )}
        </div>

        <DragOverlay dropAnimation={{ duration: 180, easing: 'cubic-bezier(0.2, 0.8, 0.2, 1)' }}>
          {dragging ? <TileFace tile={dragging.tile} size="rack" lifted /> : null}
        </DragOverlay>
      </DndContext>
    </FalseOkeyMark>
  )
}

/** A hand's end: who finished and how, their tiles, and what each lost. */
function HandOver({ game, name, now }: { game: PlainOkeyGame; name: (seat: number) => string; now: number }) {
  const { t } = useTranslation()
  const { view } = game
  const end = view.end!
  const seconds = game.nextHandAt === undefined ? undefined : Math.max(0, Math.ceil((game.nextHandAt - now) / 1000))
  const title =
    end.winner === undefined
      ? t('okey.dealtAgain')
      : end.winner === view.you
        ? t(end.finish === 'pairs' ? 'okey.youFinishedPairs' : 'okey.youFinishedSets')
        : t(end.finish === 'pairs' ? 'okey.finishedPairs' : 'okey.finishedSets', { name: name(end.winner) })
  return (
    <Overlay>
      <h2 className="text-2xl font-bold">{title}</h2>
      {end.okeyLast && <p className="mt-1 text-sm text-amber-300">{t('okey.onOkey')}</p>}
      {end.hand.length > 0 && (
        <div dir="ltr" className="mt-4 flex flex-wrap gap-1">
          {end.hand.map((tile, i) => (
            <TileFace key={i} tile={tile} size="mini" />
          ))}
        </div>
      )}
      <Points seats={view.points.map((_, seat) => seat)} points={view.points} lost={end.lost} name={name} you={view.you} />
      {seconds !== undefined && <p className="mt-4 text-sm text-slate-400">{t('okey.nextHand', { n: seconds })}</p>}
    </Overlay>
  )
}

/** The game's result: the most points left win. */
function GameOver({ game, name, leave }: { game: PlainOkeyGame; name: (seat: number) => string; leave: () => void }) {
  const { t, i18n } = useTranslation()
  const { view } = game
  const winners = view.winners ?? []
  const seats = view.points.map((_, seat) => seat).sort((a, b) => view.points[b] - view.points[a])
  return (
    <Overlay>
      <h2 className="text-2xl font-bold">
        {view.you !== undefined && winners.includes(view.you)
          ? t('results.youWon')
          : t('results.won', {
              names: new Intl.ListFormat(i18n.language, { type: 'conjunction' }).format(winners.map(name)),
              count: winners.length,
            })}
      </h2>
      <p className="mt-1 text-sm text-slate-400">{t('okey.mostPointsWin')}</p>
      <Points seats={seats} points={view.points} lost={view.end?.lost} name={name} you={view.you} />
      <Button className="mt-6 w-full" onClick={leave}>
        {t('results.leave')}
      </Button>
    </Overlay>
  )
}

function Points({
  seats,
  points,
  lost,
  name,
  you,
}: {
  seats: number[]
  points: number[]
  lost?: number[]
  name: (seat: number) => string
  you?: number
}) {
  const { t } = useTranslation()
  return (
    <table className="mt-4 w-full text-sm">
      <thead>
        <tr className="text-xs uppercase tracking-wide text-slate-500">
          <th className="pb-2 text-start font-medium" />
          {lost && <th className="pb-2 text-end font-medium">{t('okey.lost')}</th>}
          <th className="pb-2 text-end font-medium">{t('okey.points')}</th>
        </tr>
      </thead>
      <tbody>
        {seats.map((seat) => (
          <tr key={seat} className="border-t border-white/5">
            <td className="py-1.5">
              {name(seat)}
              {seat === you && <span className="text-slate-400"> · {t('board.you')}</span>}
            </td>
            {lost && (
              <td className={`py-1.5 text-end font-semibold tabular-nums ${lost[seat] > 0 ? 'text-rose-300' : 'text-slate-400'}`}>
                {lost[seat] > 0 ? `−${lost[seat]}` : '0'}
              </td>
            )}
            <td className={`py-1.5 text-end font-semibold tabular-nums ${points[seat] > 0 ? 'text-slate-100' : 'text-rose-300'}`}>
              {points[seat]}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}
