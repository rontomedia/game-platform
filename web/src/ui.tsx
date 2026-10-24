import type { ButtonHTMLAttributes, ReactNode } from 'react'

export function Panel({ title, aside, children }: { title?: string; aside?: ReactNode; children: ReactNode }) {
  return (
    <section className="rounded-2xl border border-white/10 bg-white/5 p-5 shadow-2xl backdrop-blur-xl sm:p-6">
      {(title || aside) && (
        <div className="mb-4 flex items-center justify-between gap-4">
          {title && <h2 className="text-lg font-semibold">{title}</h2>}
          {aside}
        </div>
      )}
      {children}
    </section>
  )
}

const buttonLooks = {
  primary: 'bg-teal-500 text-slate-950 hover:bg-teal-400',
  secondary: 'bg-white/10 hover:bg-white/15',
  quiet: 'text-slate-300 hover:bg-white/10',
}

export function Button({
  look = 'secondary',
  className = '',
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { look?: keyof typeof buttonLooks }) {
  return (
    <button
      {...props}
      className={`cursor-pointer rounded-xl px-4 py-2.5 font-semibold transition disabled:cursor-not-allowed disabled:opacity-40 ${buttonLooks[look]} ${className}`}
    />
  )
}

export const inputClass =
  'min-w-0 rounded-xl border border-white/10 bg-black/30 px-3 py-2.5 placeholder:text-slate-500 focus:border-teal-400/60 focus:outline-none'

/** A line of feedback: neutral, good or bad. */
export function Note({ tone, children }: { tone: 'neutral' | 'good' | 'bad'; children: ReactNode }) {
  const color = { neutral: 'text-slate-400', good: 'text-emerald-300', bad: 'text-amber-300' }[tone]
  return <p className={`text-sm font-medium ${color}`}>{children}</p>
}
