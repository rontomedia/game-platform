/**
 * Tiles travelling across the screen, with Web Animations. A tile's
 * wrapper keeps its place (dnd-kit and the layout measure it); its face is
 * what moves, or a copy of it above everything when the way crosses
 * scrolling boxes (the rack) or the tile has gone (back to a rack).
 */

export const reducedMotion = () => matchMedia('(prefers-reduced-motion: reduce)').matches

export const centre = (r: DOMRect): [number, number] => [r.left + r.width / 2, r.top + r.height / 2]

/** How long a tile takes, and when it sets off. */
export interface Pace {
  duration: number
  delay: number
}

/**
 * The `i`th of `count` tiles arriving one after another: a person's
 * quickly; a bot's more slowly, so they can be followed, all on their way
 * within a second and a half (the server shows a bot's play for longer).
 */
export function pace(i: number, count: number, bot: boolean): Pace {
  if (!bot) return { duration: 650, delay: i * 140 }
  return { duration: 1000, delay: i * Math.min(300, 1500 / Math.max(1, count - 1)) }
}

const EASING = 'cubic-bezier(0.3, 0.7, 0.2, 1)'
const LIFTED = 'scale(1.08) rotate(-3deg)'

/** A tile on the table glides to its place from `from`, a point on screen. */
export function glide(element: HTMLElement, from: [number, number], { duration, delay }: Pace) {
  const [x, y] = centre(element.getBoundingClientRect())
  if (Math.hypot(from[0] - x, from[1] - y) < 2) return
  const face = element.firstElementChild as HTMLElement
  const set = element.closest<HTMLElement>('[data-state]')
  // Above the other sets while it travels.
  if (set) set.style.zIndex = '20'
  face
    .animate(
      [
        { transform: `translate(${from[0] - x}px, ${from[1] - y}px) ${LIFTED}`, offset: 0 },
        { transform: `translate(0, 0) ${LIFTED}`, offset: 0.8 },
        { transform: 'none', offset: 1 },
      ],
      { duration, delay, easing: EASING, fill: 'backwards' },
    )
    .finished.then(() => set && (set.style.zIndex = ''))
    .catch(() => {})
}

/** A copy of `face` at `rect`, above everything, for a flight. */
function ghostOf(face: Element, rect: DOMRect): HTMLElement {
  const ghost = face.cloneNode(true) as HTMLElement
  Object.assign(ghost.style, {
    position: 'fixed',
    left: `${rect.left}px`,
    top: `${rect.top}px`,
    margin: '0',
    zIndex: '60',
    pointerEvents: 'none',
  })
  document.body.append(ghost)
  return ghost
}

/** A tile flies into its place from `from` (the pool, say), small at first. */
export function flyIn(element: HTMLElement, from: [number, number], { duration, delay }: Pace) {
  const face = element.firstElementChild as HTMLElement | null
  if (!face) return
  const rect = face.getBoundingClientRect()
  const [x, y] = centre(rect)
  const ghost = ghostOf(face, rect)
  const start = `translate(${from[0] - x}px, ${from[1] - y}px) scale(0.4) rotate(-25deg)`
  ghost
    .animate(
      [
        { transform: start, opacity: 0, offset: 0 },
        { transform: start, opacity: 1, offset: 0.05 },
        { transform: `translate(0, 0) ${LIFTED}`, opacity: 1, offset: 0.8 },
        { transform: 'none', opacity: 1, offset: 1 },
      ],
      { duration, delay, easing: EASING, fill: 'backwards' },
    )
    .finished.catch(() => {})
    .finally(() => ghost.remove())
  // The tile itself shows once its copy has landed.
  face.animate([{ opacity: 0 }, { opacity: 0 }], { duration: delay + duration })
}

/** A tile that has gone (back to a rack) flies from `rect` to `to`, shrinking away. */
export function flyAway(element: HTMLElement, rect: DOMRect, to: [number, number]) {
  const face = element.firstElementChild
  if (!face) return
  const [x, y] = centre(rect)
  const ghost = ghostOf(face, rect)
  ghost
    .animate(
      [
        { transform: 'none', opacity: 1 },
        { transform: `translate(0, -6px) ${LIFTED}`, opacity: 1, offset: 0.15 },
        { transform: `translate(${to[0] - x}px, ${to[1] - y}px) scale(0.35) rotate(12deg)`, opacity: 0 },
      ],
      { duration: 700, easing: 'cubic-bezier(0.5, 0, 0.75, 0.4)' },
    )
    .finished.catch(() => {})
    .finally(() => ghost.remove())
}
