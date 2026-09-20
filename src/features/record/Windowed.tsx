/**
 * Renders a long run of rows without mounting all of them.
 *
 * The record shows everything it holds — there are no caps, because distance is what
 * recedes and a cap would be a second, competing idea about the same thing. But "everything"
 * on a real record is tens of thousands of rows, and mounting them all is a rendering
 * problem, not a design one.
 *
 * So the rows are split into chunks, and a chunk that is nowhere near the viewport is
 * replaced by a spacer of exactly the height it last measured. Nothing about the layout
 * changes: the scrollbar is the right length from the first paint onward, the column does
 * not jump, and scrolling past a chunk brings it back at the same height it left.
 *
 * Chunk granularity rather than row granularity is deliberate. Rows inside a band are close
 * to uniform in height, so one measurement stands in for the whole chunk; and the observer
 * count stays proportional to length/CHUNK rather than to length.
 */

import { useEffect, useRef, useState, type ReactNode } from "react";

/**
 * Rows per chunk.
 *
 * Small enough that a chunk's height is dominated by rows of one tier, large enough that a
 * record of 50 000 fragments needs hundreds of observers rather than thousands.
 */
const CHUNK = 50;

/**
 * How far outside the viewport a chunk still counts as near.
 *
 * Generously more than a screen in both directions, so a chunk is always mounted and
 * measured before it can be seen, and a fast scroll does not outrun the observer.
 */
const NEAR = "1400px";

/**
 * Below this there is nothing to gain: the whole run costs less than the bookkeeping.
 * Most bands are far smaller than one chunk, and this keeps them on the simple path.
 */
const WINDOW_ABOVE = CHUNK * 2;

export interface WindowedProps<T> {
  items: T[];
  /** Stable per item; a chunk's identity must not change when items are prepended. */
  keyOf: (item: T) => string;
  children: (item: T, index: number) => ReactNode;
  /**
   * A best guess at one row's height, used only for a chunk that has never been mounted.
   * It is replaced by the real measurement the first time the chunk comes near.
   */
  estimatedRowHeight: number;
  /**
   * The gap the band puts between its rows.
   *
   * A chunk is a real box, so it has to reproduce that gap internally — then the gap
   * between two chunks and the gap between two rows are the same measurement, and chunking
   * is invisible in the layout.
   */
  gap: string;
}

export function Windowed<T>({ items, keyOf, children, estimatedRowHeight, gap }: WindowedProps<T>) {
  if (items.length <= WINDOW_ABOVE) {
    return <>{items.map((item, i) => children(item, i))}</>;
  }

  const chunks: T[][] = [];
  for (let i = 0; i < items.length; i += CHUNK) chunks.push(items.slice(i, i + CHUNK));

  return (
    <>
      {chunks.map((chunk, ci) => (
        <Chunk
          key={keyOf(chunk[0])}
          // The first chunk is always mounted: the top of a band is what you arrive at.
          initiallyNear={ci === 0}
          estimatedHeight={chunk.length * estimatedRowHeight}
          gap={gap}
        >
          {chunk.map((item, i) => children(item, ci * CHUNK + i))}
        </Chunk>
      ))}
    </>
  );
}

function Chunk({
  children,
  initiallyNear,
  estimatedHeight,
  gap,
}: {
  children: ReactNode;
  initiallyNear: boolean;
  estimatedHeight: number;
  gap: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [near, setNear] = useState(initiallyNear);
  /** The last height this chunk actually occupied, so unmounting it moves nothing. */
  const measured = useRef<number | null>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    // No IntersectionObserver (jsdom, an old webview): mount everything rather than
    // silently rendering a column of empty spacers.
    if (typeof IntersectionObserver === "undefined") {
      setNear(true);
      return;
    }
    const io = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          setNear(true);
        } else {
          // Measure before letting go, never after.
          const h = el.getBoundingClientRect().height;
          if (h > 0) measured.current = h;
          setNear(false);
        }
      },
      { rootMargin: `${NEAR} 0px ${NEAR} 0px` },
    );
    io.observe(el);
    return () => io.disconnect();
  }, []);

  return (
    <div
      ref={ref}
      style={{
        display: "flex",
        flexDirection: "column",
        gap,
        ...(near ? null : { height: measured.current ?? estimatedHeight, overflow: "hidden" }),
      }}
    >
      {near ? children : null}
    </div>
  );
}
