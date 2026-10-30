import { LayoutGroup, motion } from 'motion/react'
import { useId, useRef, type ComponentType } from 'react'
import { useTranslation } from 'react-i18next'

import { HowToPlay as Rules, RuleList, RuleSection, RuleText } from '../../platform/HowToPlay.tsx'
import { useScene } from '../../platform/useScene.ts'

import { TileFace } from './TileView.tsx'
import type { Color, RuleSet, Tile } from './wasm/engine.js'

/**
 * How to play each rule set: a few short sections, most with a little scene
 * of real tiles. A rule set without an entry gets no "How to play" button.
 */
const RULES: Partial<Record<RuleSet['kind'], { name: `home.${RuleSet['kind']}`; Content: ComponentType }>> = {
  classic: { name: 'home.classic', Content: ClassicRules },
  okey101: { name: 'home.okey101', Content: Okey101Rules },
  okey: { name: 'home.okey', Content: PlainOkeyRules },
}

/** "How to play", unfolding the rules of `kind` below it. */
export function HowToPlay({ kind }: { kind: RuleSet['kind'] }) {
  const { t } = useTranslation()
  const rules = RULES[kind]
  if (!rules) return null
  return (
    <Rules game={t(rules.name)}>
      <rules.Content />
    </Rules>
  )
}

const Section = RuleSection
const Text = RuleText

function ClassicRules() {
  const { t } = useTranslation()
  const buttons = { draw: t('board.draw'), endTurn: t('board.endTurn'), reset: t('board.reset') }
  return (
    <>
      <Section title={t('rules.classic.aimTitle')}>
        <Text>{t('rules.classic.aim')}</Text>
        <figure dir="ltr" className="felt mt-3 flex justify-center gap-0.5 rounded-xl p-3">
          {['k1', 'b5', 'o9', 'r13', 'j'].map((id) => (
            <TileFace key={id} tile={tileOf(id)} size="guide" />
          ))}
        </figure>
      </Section>
      <Section title={t('rules.classic.setsTitle')}>
        <Text>{t('rules.classic.sets')}</Text>
        <Sets
          sets={[
            [t('rules.classic.run'), ['b3', 'b4', 'b5', 'b6']],
            [t('rules.classic.group'), ['k7', 'r7', 'b7', 'o7']],
          ]}
        />
      </Section>
      <Section title={t('rules.classic.turnTitle')}>
        <Text>{t('rules.classic.turn', buttons)}</Text>
        <Scene
          caption={t('rules.classic.turnCaption')}
          frames={[
            { table: [['b4', 'b5', 'b6'], ['k8', 'o8', 'b8']], rack: ['b3', 'k12', 'r8', 'o2'] },
            { table: [['b3', 'b4', 'b5', 'b6'], ['k8', 'o8', 'b8']], rack: ['k12', 'r8', 'o2'] },
            { table: [['b3', 'b4', 'b5', 'b6'], ['k8', 'o8', 'b8', 'r8']], rack: ['k12', 'o2'] },
          ]}
        />
      </Section>
      <Section title={t('rules.classic.firstTitle')}>
        <Text>{t('rules.classic.first')}</Text>
        <Scene
          caption={t('rules.classic.firstCaption')}
          frames={[
            { table: [], rack: ['k2', 'r10', 'o7', 'r11', 'r12'] },
            { table: [['r10', 'r11', 'r12']], rack: ['k2', 'o7'], note: '10 + 11 + 12 = 33' },
          ]}
        />
      </Section>
      <Section title={t('rules.classic.tableTitle')}>
        <Text>{t('rules.classic.table')}</Text>
        <Scene
          caption={t('rules.classic.splitCaption')}
          frames={[
            { table: [['r4', 'r5', 'r6', 'r7', 'r8']], rack: ['r6.2', 'k1'] },
            { table: [['r4', 'r5', 'r6'], ['r7', 'r8']], rack: ['r6.2', 'k1'] },
            { table: [['r4', 'r5', 'r6'], ['r6.2', 'r7', 'r8']], rack: ['k1'] },
          ]}
        />
        <Scene
          caption={t('rules.classic.borrowCaption')}
          frames={[
            { table: [['b4', 'r4', 'k4', 'o4']], rack: ['b3', 'b5', 'b6'] },
            { table: [['r4', 'k4', 'o4'], ['b4']], rack: ['b3', 'b5', 'b6'] },
            { table: [['r4', 'k4', 'o4'], ['b3', 'b4', 'b5', 'b6']], rack: [] },
          ]}
        />
      </Section>
      <Section title={t('rules.classic.jokerTitle')}>
        <Text>{t('rules.classic.joker')}</Text>
        <Scene
          caption={t('rules.classic.jokerCaption')}
          frames={[
            { table: [['r3', 'b3', 'j']], rack: ['k3', 'o5', 'o6'] },
            { table: [['r3', 'b3', 'k3'], ['j']], rack: ['o5', 'o6'] },
            { table: [['r3', 'b3', 'k3'], ['o5', 'o6', 'j']], rack: [] },
          ]}
        />
      </Section>
      <Section title={t('rules.classic.endTitle')}>
        <Text>{t('rules.classic.end')}</Text>
      </Section>
      <Section title={t('rules.classic.controlsTitle')}>
        <RuleList items={[t('rules.classic.controlsDrag'), t('rules.classic.controlsSets'), t('rules.classic.controlsPlans'), t('rules.classic.controlsReset', buttons), t('rules.classic.controlsTime')]} />
      </Section>
    </>
  )
}

/** The okey in the Okey scenes: the indicator is a red 4. */
const OKEY = 'r5'

/** A run, group or pair of Okey 101: three or more, or two the same. */
const okeyValid = (set: string[]) => set.length >= 3 || (set.length === 2 && faceOf(set[0]) === faceOf(set[1]))

/** The indicator, the okey it picks, and a false okey: the same in both Okey rule sets. */
function IndicatorSection() {
  const { t } = useTranslation()
  return (
    <Section title={t('rules.okeyShared.indicatorTitle')}>
      <Text>{t('rules.okeyShared.indicator')}</Text>
      <Sets
        okey={OKEY}
        sets={[
          [t('okey.indicator'), ['r4']],
          [t('okey.okey'), ['r5', 'r5.2']],
          [t('rules.okeyShared.falseOkey'), ['j', 'j.2']],
        ]}
        caption={t('rules.okeyShared.indicatorCaption')}
      />
    </Section>
  )
}

function Okey101Rules() {
  const { t } = useTranslation()
  const words = {
    draw: t('board.draw'),
    reset: t('board.reset'),
    rising: t('home.progressive'),
    penalties: t('home.penalties'),
  }
  return (
    <>
      <Section title={t('rules.okey101.aimTitle')}>
        <Text>{t('rules.okey101.aim')}</Text>
      </Section>
      <IndicatorSection />
      <Section title={t('rules.okey101.setsTitle')}>
        <Text>{t('rules.okey101.sets')}</Text>
        <Sets
          okey={OKEY}
          sets={[
            [t('rules.okeyShared.run'), ['o3', 'o4', 'o5', 'o6']],
            [t('rules.okeyShared.group'), ['k7', 'r7', 'b7']],
            [t('rules.okeyShared.pair'), ['b11', 'b11.2']],
            [t('rules.okeyShared.withOkey'), ['k8', 'r5', 'k10']],
          ]}
        />
      </Section>
      <Section title={t('rules.okey101.turnTitle')}>
        <Text>{t('rules.okey101.turn', words)}</Text>
        <Scene
          okey={OKEY}
          valid={okeyValid}
          caption={t('rules.okey101.turnCaption')}
          frames={[
            { table: [['b4', 'b5', 'b6']], taken: 'b7', rack: ['b8', 'o2', 'k13'] },
            { table: [['b4', 'b5', 'b6', 'b7']], rack: ['b8', 'o2', 'k13'] },
            { table: [['b4', 'b5', 'b6', 'b7', 'b8']], rack: ['o2', 'k13'] },
            { table: [['b4', 'b5', 'b6', 'b7', 'b8']], rack: ['k13'], discard: 'o2' },
          ]}
        />
      </Section>
      <Section title={t('rules.okey101.openTitle')}>
        <Text>{t('rules.okey101.open', words)}</Text>
        <Scene
          okey={OKEY}
          valid={okeyValid}
          caption={t('rules.okey101.openCaption')}
          frames={[
            { table: [], rack: ['k10', 'r9', 'k11', 'b9', 'o11', 'k12', 'b2', 'o9', 'b11', 'k13', 'r11'] },
            {
              table: [
                ['k10', 'k11', 'k12', 'k13'],
                ['r9', 'b9', 'o9'],
                ['o11', 'b11', 'r11'],
              ],
              rack: ['b2'],
              note: '46 + 27 + 33 = 106',
            },
          ]}
        />
        <Scene
          okey={OKEY}
          valid={okeyValid}
          caption={t('rules.okey101.pairsCaption')}
          frames={[
            { table: [], rack: ['k3', 'o8', 'b12', 'k3.2', 'r1', 'o8.2', 'b6', 'r1.2', 'b12.2', 'b6.2', 'k9'] },
            {
              table: [
                ['k3', 'k3.2'],
                ['o8', 'o8.2'],
                ['b12', 'b12.2'],
                ['r1', 'r1.2'],
                ['b6', 'b6.2'],
              ],
              rack: ['k9'],
            },
          ]}
        />
      </Section>
      <Section title={t('rules.okey101.addTitle')}>
        <Text>{t('rules.okey101.add')}</Text>
        <Scene
          okey={OKEY}
          valid={okeyValid}
          caption={t('rules.okey101.addCaption')}
          frames={[
            { table: [['b4', 'b5', 'b6'], ['k9', 'o9', 'b9', 'r5']], rack: ['b3', 'b7', 'r9', 'o2'] },
            { table: [['b3', 'b4', 'b5', 'b6', 'b7'], ['k9', 'o9', 'b9', 'r5']], rack: ['r9', 'o2'] },
            { table: [['b3', 'b4', 'b5', 'b6', 'b7'], ['k9', 'o9', 'b9', 'r9']], rack: ['r5', 'o2'] },
          ]}
        />
      </Section>
      <Section title={t('rules.okey101.penaltiesTitle')}>
        <Text>{t('rules.okey101.penalties', words)}</Text>
      </Section>
      <Section title={t('rules.okey101.outTitle')}>
        <Text>{t('rules.okey101.out')}</Text>
        <Scene
          okey={OKEY}
          valid={okeyValid}
          caption={t('rules.okey101.outCaption')}
          frames={[
            { table: [['r7', 'b7', 'k7']], rack: ['k1', 'k2', 'k3', 'o12'] },
            { table: [['r7', 'b7', 'k7'], ['k1', 'k2', 'k3']], rack: ['o12'] },
            { table: [['r7', 'b7', 'k7'], ['k1', 'k2', 'k3']], rack: [], discard: 'o12' },
          ]}
        />
      </Section>
      <Section title={t('rules.okey101.scoresTitle')}>
        <Text>{t('rules.okey101.scores')}</Text>
      </Section>
      <Section title={t('rules.okeyShared.controlsTitle')}>
        <RuleList items={[t('rules.okey101.controlsDraw', words), t('rules.okey101.controlsSets'), t('rules.classic.controlsSets'), t('rules.classic.controlsPlans'), t('rules.okey101.controlsDiscard', words), t('rules.okeyShared.controlsNext'), t('rules.okey101.controlsTime')]} />
      </Section>
    </>
  )
}

function PlainOkeyRules() {
  const { t } = useTranslation()
  const words = { draw: t('board.draw') }
  return (
    <>
      <Section title={t('rules.okey.aimTitle')}>
        <Text>{t('rules.okey.aim')}</Text>
      </Section>
      <IndicatorSection />
      <Section title={t('rules.okey.handTitle')}>
        <Text>{t('rules.okey.hand')}</Text>
        <Sets
          okey={OKEY}
          sets={[
            [t('rules.okeyShared.run'), ['r12', 'r13', 'r1']],
            [t('rules.okeyShared.run'), ['k5', 'k6', 'k7', 'k8'], 'run2'],
            [t('rules.okeyShared.group'), ['b9', 'o9', 'k9', 'r9']],
            [t('rules.okeyShared.withOkey'), ['o3', 'r5', 'o5']],
          ]}
          caption={t('rules.okey.handCaption')}
        />
      </Section>
      <Section title={t('rules.okey.turnTitle')}>
        <Text>{t('rules.okey.turn', words)}</Text>
        <Scene
          okey={OKEY}
          caption={t('rules.okey.turnCaption')}
          frames={[
            { table: [], taken: 'o6', rack: ['o4', 'o5', 'k2', 'b11', 'b11.2'] },
            { table: [], rack: ['o4', 'o5', 'o6', 'k2', 'b11', 'b11.2'] },
            { table: [], rack: ['o4', 'o5', 'o6', 'b11', 'b11.2'], discard: 'k2' },
          ]}
        />
      </Section>
      <Section title={t('rules.okey.finishTitle')}>
        <Text>{t('rules.okey.finish')}</Text>
      </Section>
      <Section title={t('rules.okey.scoresTitle')}>
        <Text>{t('rules.okey.scores')}</Text>
      </Section>
      <Section title={t('rules.okeyShared.controlsTitle')}>
        <RuleList items={[t('rules.okey.controlsDraw', words), t('rules.okey.controlsDiscard'), t('rules.okeyShared.controlsNext'), t('rules.okey.controlsTime')]} />
      </Section>
    </>
  )
}

const COLORS: Record<string, Color> = { k: 'black', b: 'blue', o: 'orange', r: 'red' }

/** A scene's tile without its twin mark: `b4` is a blue 4, `j` a joker; `.2` tells twins apart. */
const faceOf = (id: string) => id.split('.')[0]

function tileOf(id: string): Tile {
  const face = faceOf(id)
  return face === 'j' ? { kind: 'joker' } : { kind: 'number', color: COLORS[face[0]], value: Number(face.slice(1)) }
}

/** A tile in a picture, ringed as on the Okey boards when it's the okey. */
function Face({ id, okey }: { id: string; okey?: string }) {
  const face = <TileFace tile={tileOf(id)} size="guide" />
  return faceOf(id) === okey ? <div className="rounded-[5px] ring-2 ring-amber-300/80">{face}</div> : face
}

/** Still sets side by side, each named under it (a third entry is a key, when names repeat). */
function Sets({
  sets,
  okey,
  caption,
}: {
  sets: (readonly [string, string[]] | readonly [string, string[], string])[]
  okey?: string
  caption?: string
}) {
  return (
    <figure className="mt-3">
      <div dir="ltr" className="felt flex flex-wrap justify-around gap-3 rounded-xl p-3">
        {sets.map(([name, set, key]) => (
          <div key={key ?? name} className="flex flex-col items-center gap-1.5">
            <div className="flex gap-0.5">
              {set.map((id) => (
                <Face key={id} id={id} okey={okey} />
              ))}
            </div>
            <p dir="auto" className="text-xs font-medium text-emerald-100/70">
              {name}
            </p>
          </div>
        ))}
      </div>
      {caption && <figcaption className="mt-1.5 text-xs text-slate-400">{caption}</figcaption>}
    </figure>
  )
}

/** The table's sets and the rack at one moment of a scene. */
interface Frame {
  table: string[][]
  rack: string[]
  /** Okey: the previous player's discard, on offer left of the rack. */
  taken?: string
  /** Okey: the tile discarded, right of the rack. */
  discard?: string
  /** A line shown under the table, such as a sum. */
  note?: string
}

/**
 * A little table and rack whose tiles glide from one frame to the next,
 * over and over while it's in view. Outlines as on the board: amber for a set
 * not `valid` (by default, too short), green for one that wasn't there at the
 * start. With reduced motion, the last frame.
 */
function Scene({
  frames,
  caption,
  okey,
  valid = (set) => set.length >= 3,
}: {
  frames: Frame[]
  caption?: string
  okey?: string
  valid?: (set: string[]) => boolean
}) {
  const ref = useRef<HTMLElement>(null)
  const { step, round } = useScene(ref, frames.length)
  const group = useId()

  const frame = frames[step]
  const before = new Set(frames[0].table.map((set) => set.join()))
  const hasTable = frames.some((f) => f.table.length > 0)
  const hasRack = frames.some((f) => f.rack.length > 0)
  const hasNote = frames.some((f) => f.note)
  const hasTaken = frames.some((f) => f.taken)
  const hasDiscard = frames.some((f) => f.discard)
  return (
    <figure ref={ref} className="mt-3">
      <div dir="ltr" className="felt overflow-hidden rounded-xl p-3" aria-hidden>
        <LayoutGroup id={group}>
          {/* A new round fades in afresh rather than every tile flying back. */}
          <motion.div key={round} initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ duration: 0.4 }}>
            {hasTable && (
              <div className="mb-3 flex min-h-[var(--scene-row)] flex-wrap content-start items-start gap-2">
                {frame.table.map((set) => (
                  <div
                    key={set.join()}
                    className={`flex gap-0.5 rounded-lg p-1 transition-shadow duration-300 ${
                      !valid(set)
                        ? 'bg-amber-400/15 shadow-[0_0_0_2px_rgb(251_191_36/0.7)]'
                        : before.has(set.join())
                          ? ''
                          : 'bg-emerald-400/15 shadow-[0_0_0_2px_rgb(52_211_153/0.75)]'
                    }`}
                  >
                    {set.map((id) => (
                      <Piece key={id} id={id} okey={okey} />
                    ))}
                  </div>
                ))}
              </div>
            )}
            {hasNote && <p className="-mt-3 mb-3 h-5 text-center text-sm font-semibold tabular-nums text-emerald-100/80">{frame.note}</p>}
            {hasRack && (
              <div className="flex items-center gap-2">
                {hasTaken && <Spot id={frame.taken} okey={okey} />}
                <div className="tray flex min-h-[var(--scene-row)] min-w-0 flex-1 flex-wrap gap-0.5 rounded-lg p-1">
                  {frame.rack.map((id) => (
                    <Piece key={id} id={id} okey={okey} />
                  ))}
                </div>
                {hasDiscard && <Spot id={frame.discard} okey={okey} />}
              </div>
            )}
          </motion.div>
        </LayoutGroup>
      </div>
      {caption && <figcaption className="mt-1.5 text-xs text-slate-400">{caption}</figcaption>}
    </figure>
  )
}

function Piece({ id, okey }: { id: string; okey?: string }) {
  return (
    <motion.div layoutId={id} transition={{ type: 'spring', stiffness: 260, damping: 28 }}>
      <Face id={id} okey={okey} />
    </motion.div>
  )
}

/** A discard spot beside the rack, holding a tile or empty. */
function Spot({ id, okey }: { id?: string; okey?: string }) {
  return (
    <div className="grid min-h-[var(--scene-row)] min-w-9 place-items-center rounded-lg border border-dashed border-white/25 p-1 sm:min-w-11">
      {id && <Piece id={id} okey={okey} />}
    </div>
  )
}
