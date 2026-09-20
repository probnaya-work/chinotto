/**
 * The record: everything you have, arranged by distance.
 *
 * Not a feed and not a list of slots. `bandsFor()` walks the material once in time order
 * and opens a band wherever the distance changes, so a tier can recur down the column and
 * the reading order is always chronological. Nothing is capped — distance is what recedes,
 * and a cap would be a second idea about the same thing — so long bands are windowed
 * instead (see `Windowed`).
 *
 * The column, top to bottom: the edge, a return, whatever is held, then the record.
 * Held material sits above the record rather than inside it because it has been pulled out
 * of time on purpose; putting it back in the flow would undo the only thing holding does.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import type { Fragment, HeldFragment, Materials } from "../../lib/recordApi";
import { TIERS, bandLabelStyle, metaStyle } from "../../design/tiers";
import {
  bandAnchor,
  bandLabelText,
  bandsFor,
  type Anchor,
  type Band,
  type MaterialBand,
} from "./bands";
import { CompactRow } from "./CompactRow";
import { FragmentRow } from "./FragmentRow";
import { Windowed } from "./Windowed";
import { YearRow } from "./YearRow";
import { heldSinceLabel } from "./format";

/**
 * A guess at one row's height per tier, used only for a chunk that has never been on
 * screen. Replaced by a real measurement the moment it is.
 */
const ROW_ESTIMATE = { d0: 96, d1: 40, d2: 26, d3: 22, d4: 20 } as const;

export interface RecordProps {
  fragments: Fragment[];
  held: HeldFragment[];
  now: Date;
  materials?: Materials;
  /** Standing in a month: distance is measured from there instead of from today. */
  anchor?: Anchor | null;
  /** Find's query. Filters before banding, so the bands describe the result. */
  query?: string | null;

  /** Until the first read resolves we do not know whether the record is empty. */
  loaded: boolean;

  onOpen: (f: Fragment) => void;
  onContinue: (f: Fragment) => void;
  onHold: (f: Fragment) => void;
  onRelease: (f: Fragment) => void;
  onStandIn: (year: number, month: number) => void;
  onCorrect: (f: Fragment) => void;
  onRemove: (f: Fragment) => void;
  onCopy: (f: Fragment) => void;
  onCopyLink: (f: Fragment) => void;
  onPlay?: (f: Fragment) => void;
  playingId?: string | null;

  /** The fragment just left, and what the edit window still says about it. */
  justSaved?: {
    id: string;
    secondsLeft: number;
    landing: boolean;
    suggestion: { text: string; when: string } | null;
  } | null;
  onAcceptSuggestion?: () => void;
  onRejectSuggestion?: () => void;

  /** Correcting, driven from the surface so ⋯ and the edit window can both start it. */
  editingId?: string | null;
  editText?: string;
  onEditChange?: (v: string) => void;
  onEditSave?: () => void;
  onEditCancel?: () => void;

  /** "↳ moment 3 of a line · since march", by fragment id. */
  lineMeta?: Map<string, string>;

  /** Keyboard: ↓ from the edge lands here. */
  keyboardActive?: boolean;
  onLeaveKeyboard?: () => void;

  /** Rendered between the edge and the record. */
  returnSlot?: React.ReactNode;
}

export function Record(props: RecordProps) {
  const {
    fragments,
    held,
    now,
    materials,
    anchor = null,
    query = null,
    loaded,
    justSaved = null,
    editingId = null,
    lineMeta,
    keyboardActive = false,
    returnSlot,
  } = props;

  const [hovered, setHovered] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [menuFor, setMenuFor] = useState<string | null>(null);
  const [provenanceFor, setProvenanceFor] = useState<Set<string>>(() => new Set());
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());

  const heldIds = useMemo(() => new Set(held.map((h) => h.fragment.id)), [held]);

  // Held material is lifted out of the record — but only at the edge. Standing in a month
  // or filtering shows the record as it actually is, not as the edge arranges it.
  const atEdge = !anchor && !query;

  const bands = useMemo(
    () =>
      bandsFor({
        fragments,
        now,
        anchor,
        query,
        exclude: atEdge ? heldIds : null,
      }),
    [fragments, now, anchor, query, atEdge, heldIds],
  );

  /** Reading order, for ↑↓. Only D0 is navigable: it is the only tier with verbs. */
  const navigable = useMemo(
    () =>
      bands
        .filter((b): b is MaterialBand => b.kind === "material" && b.tier === "d0")
        .flatMap((b) => b.items),
    [bands],
  );

  const move = useCallback(
    (delta: number) => {
      if (navigable.length === 0) return;
      // Functional update, not a read of `selected`: held arrow keys fire faster than
      // React re-renders, so reading from the closure collapses a burst into one step.
      setSelected((current) => {
        const i = current ? navigable.findIndex((f) => f.id === current) : -1;
        if (i === 0 && delta < 0) {
          props.onLeaveKeyboard?.();
          return null;
        }
        const next = Math.min(navigable.length - 1, Math.max(0, i + delta));
        return navigable[next]?.id ?? null;
      });
    },
    [navigable, props],
  );

  useEffect(() => {
    if (!keyboardActive) {
      setSelected(null);
      return;
    }
    setSelected((s) => s ?? navigable[0]?.id ?? null);
  }, [keyboardActive, navigable]);

  useEffect(() => {
    if (!keyboardActive) return;
    function onKey(e: KeyboardEvent) {
      const current = navigable.find((f) => f.id === selected);
      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          move(1);
          break;
        case "ArrowUp":
          e.preventDefault();
          move(-1);
          break;
        case "Enter":
          if (current) {
            e.preventDefault();
            props.onOpen(current);
          }
          break;
        case "c":
          if (current) {
            e.preventDefault();
            props.onContinue(current);
          }
          break;
        case "h":
          if (current) {
            e.preventDefault();
            props.onHold(current);
          }
          break;
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [keyboardActive, move, navigable, selected, props]);

  const toggleIn = (set: Set<string>, id: string) => {
    const next = new Set(set);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    return next;
  };

  function renderBand(band: Band) {
    if (band.kind === "years") {
      return (
        <div
          key={band.key}
          role="list"
          aria-label="earlier years"
          style={{
            marginTop: "var(--band-top-years)",
            paddingLeft: "var(--indent)",
            display: "flex",
            flexDirection: "column",
            gap: "8px",
          }}
        >
          {band.years.map((y) => (
            <YearRow
              key={y.year}
              year={y.year}
              density={y.months}
              count={y.count}
              firstLines={y.firstLines}
              onStandIn={props.onStandIn}
            />
          ))}
        </div>
      );
    }

    const tier = TIERS[band.tier];
    const label = bandLabelText(band, now, anchor);

    return (
      <div
        key={band.key}
        role="list"
        aria-label={label || "the last few hours"}
        style={{
          marginTop: tier.bandTop,
          paddingLeft: tier.indent,
          display: "flex",
          flexDirection: "column",
          gap: tier.gap,
        }}
      >
        {label ? (
          <button
            type="button"
            className="chinotto-band-label"
            aria-label={`stand in ${label}`}
            style={{
              ...bandLabelStyle(band.tier),
              background: "none",
              border: "none",
              padding: 0,
              font: "inherit",
              textAlign: "left",
              alignSelf: "flex-start",
            }}
            onClick={() => {
              const target = bandAnchor(band);
              if (target) props.onStandIn(target.year, target.month);
            }}
          >
            {label}
          </button>
        ) : null}

        <Windowed
          items={band.items}
          keyOf={(f) => f.id}
          gap={tier.gap}
          estimatedRowHeight={ROW_ESTIMATE[band.tier]}
        >
          {(f) =>
            band.tier === "d0" ? (
              <FragmentRow
                key={f.id}
                fragment={f}
                now={now}
                encounter={materials?.encounters.get(f.id)}
                voice={materials?.voices.get(f.id)}
                mark={query}
                lineMeta={lineMeta?.get(f.id) ?? null}
                active={hovered === f.id}
                selected={selected === f.id}
                held={heldIds.has(f.id)}
                onHover={setHovered}
                onOpen={props.onOpen}
                onContinue={props.onContinue}
                onHold={(x) => (heldIds.has(x.id) ? props.onRelease(x) : props.onHold(x))}
                gutterOverride={justSaved?.id === f.id ? "now" : undefined}
                menuOpen={menuFor === f.id}
                onToggleMenu={(x) => setMenuFor((m) => (m === x.id ? null : x.id))}
                onCorrect={(x) => {
                  setMenuFor(null);
                  props.onCorrect(x);
                }}
                onCopy={(x) => {
                  setMenuFor(null);
                  props.onCopy(x);
                }}
                onCopyLink={(x) => {
                  setMenuFor(null);
                  props.onCopyLink(x);
                }}
                onRemove={(x) => {
                  setMenuFor(null);
                  props.onRemove(x);
                }}
                provenanceOpen={provenanceFor.has(f.id)}
                onToggleProvenance={(x) => {
                  setMenuFor(null);
                  setProvenanceFor((s) => toggleIn(s, x.id));
                }}
                expanded={expanded.has(f.id)}
                onExpand={(x) => setExpanded((s) => toggleIn(s, x.id))}
                editing={editingId === f.id}
                editText={props.editText}
                onEditChange={props.onEditChange}
                onEditSave={props.onEditSave}
                onEditCancel={props.onEditCancel}
                justSaved={justSaved?.id === f.id}
                landing={justSaved?.id === f.id && justSaved.landing}
                secondsLeft={justSaved?.secondsLeft ?? 0}
                suggestion={justSaved?.id === f.id ? justSaved.suggestion : null}
                onAcceptSuggestion={props.onAcceptSuggestion}
                onRejectSuggestion={props.onRejectSuggestion}
                onPlay={props.onPlay}
                playing={props.playingId === f.id}
              />
            ) : (
              <CompactRow
                key={f.id}
                fragment={f}
                tier={band.tier}
                onOpen={props.onOpen}
                mark={query ?? undefined}
                encounter={materials?.encounters.get(f.id)}
                voice={materials?.voices.get(f.id)}
              />
            )
          }
        </Windowed>
      </div>
    );
  }

  const recordIsEmpty = loaded && fragments.length === 0 && held.length === 0;

  return (
    <>
      {returnSlot}

      {atEdge && held.length > 0 ? (
        <div style={{ marginTop: "34px", display: "flex", flexDirection: "column", gap: "18px" }}>
          {held.map(({ fragment: f, heldAt }) => (
            <div
              key={f.id}
              style={{
                paddingLeft: "var(--indent)",
                position: "relative",
                fontSize: "var(--size-held)",
                lineHeight: 1.3,
                color: "var(--ink-near)",
                fontVariationSettings: "'wdth' 96",
              }}
            >
              <span
                aria-hidden="true"
                style={{
                  position: "absolute",
                  left: "var(--gutter-width)",
                  top: 0,
                  bottom: 0,
                  width: "2px",
                  background: "var(--rule)",
                }}
              />
              <span style={{ cursor: "pointer" }} onClick={() => props.onOpen(f)}>
                {f.body}
              </span>
              <span style={{ ...metaStyle(), display: "block", marginTop: "4px" }}>
                {heldSinceLabel(new Date(heldAt), now)} ·{" "}
                <button
                  type="button"
                  className="chinotto-verb chinotto-verb--quiet"
                  onClick={() => props.onRelease(f)}
                  style={{
                    background: "none",
                    border: "none",
                    padding: 0,
                    font: "inherit",
                    color: "var(--agency-quiet)",
                    cursor: "pointer",
                  }}
                >
                  release
                </button>
              </span>
            </div>
          ))}
        </div>
      ) : null}

      {/*
        The only instruction the product gives — and it must not flash before the first read
        resolves, or every launch says the record is empty and then fills it in.
      */}
      {recordIsEmpty ? (
        <div
          style={{
            marginTop: "22px",
            display: "flex",
            gap: "var(--gutter-gap)",
            fontSize: "var(--size-d0)",
            lineHeight: 1.24,
            letterSpacing: "var(--track-d0)",
            color: "var(--meta)",
          }}
        >
          <span style={{ width: "var(--gutter-width)", flex: "none" }} />
          <div>type anything and press return. it lands here, and stays.</div>
        </div>
      ) : null}

      {bands.map(renderBand)}
    </>
  );
}
