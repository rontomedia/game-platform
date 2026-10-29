import { useLayoutEffect, useState, type RefObject } from 'react'

/** The size of a table cell, and how many fit across (set in index.css, `.table-grid`). */
export interface Metrics {
  cellW: number
  cellH: number
  columns: number
}

/** The table grid's cell size and width, measured again whenever it's resized. */
export function useTableMetrics(grid: RefObject<HTMLDivElement | null>): Metrics | null {
  const [metrics, setMetrics] = useState<Metrics | null>(null)
  useLayoutEffect(() => {
    const element = grid.current
    if (!element) return
    const measure = () => {
      const style = getComputedStyle(element)
      const cellW = parseFloat(style.getPropertyValue('--cell-w'))
      const cellH = parseFloat(style.getPropertyValue('--cell-h'))
      const columns = Math.max(4, Math.floor(element.clientWidth / cellW))
      setMetrics((m) => (m && m.cellW === cellW && m.cellH === cellH && m.columns === columns ? m : { cellW, cellH, columns }))
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(element)
    return () => observer.disconnect()
  }, [grid])
  return metrics
}
