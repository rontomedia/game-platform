import { useInView, useReducedMotion } from 'motion/react'
import { useEffect, useState, type RefObject } from 'react'

/** A scene's first frame a while, the next ones less, the last longest. */
const PAUSE = (step: number, last: boolean) => (step === 0 ? 1400 : last ? 2800 : 1300)

/**
 * A scene's frames, played over and over while it's in view (`ref` on
 * it): the frame to show, and the round (a new one starts afresh). With
 * reduced motion, the last frame. `pause` gives each frame's time in ms.
 */
export function useScene(
  ref: RefObject<Element | null>,
  frames: number,
  pause: (step: number, last: boolean) => number = PAUSE,
): { step: number; round: number } {
  const reduced = useReducedMotion()
  const inView = useInView(ref, { amount: 0.6 })
  const [step, setStep] = useState(0)
  const [round, setRound] = useState(0)
  useEffect(() => {
    if (reduced || !inView || frames < 2) return
    const last = step === frames - 1
    const timer = setTimeout(() => {
      if (!last) return setStep(step + 1)
      setStep(0)
      setRound((round) => round + 1)
    }, pause(step, last))
    return () => clearTimeout(timer)
  }, [step, inView, reduced, frames, pause])
  return { step: reduced ? frames - 1 : step, round }
}
