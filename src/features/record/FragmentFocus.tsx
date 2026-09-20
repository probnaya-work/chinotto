/**
 * Focus — a fragment, or the line it turned out to be part of.
 *
 * This surface exists because preservation has to be *visible*. Every moment keeps its own
 * date and its own wording; continuing adds a new moment at the end, dated now; correcting
 * changes wording in place and says so on the spot. There is no generic edit mode and no
 * version-control vocabulary — the distinction between original, continuation and
 * correction is carried by the layout, not by a label.
 *
 * It replaces the column rather than opening on top of it. `esc` goes back to the edge.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as api from "../../lib/recordApi";
import type { Fragment, LineMoment, Materials } from "../../lib/recordApi";
import { metaStyle } from "../../design/tiers";
import { useNow } from "./useNow";
import { Marked } from "./Marked";
import { assembleTraces, tracesFor, tracesHeading, type Trace } from "./traces";
import { snippetAround } from "./words";
import {
  clockLabel,
  dayLabel,
  durationLabel,
  fullDateLabel,
  monthLabel,
} from "./format";
import { Verb } from "./Verb";

/** Moments kept at each end when a line folds. */
const KEEP_HEAD = 2;
const KEEP_TAIL = 2;

/** Past this, reading a line end to end stops being the point. */
const FOLD_ABOVE = KEEP_HEAD + KEEP_TAIL + 1;

export interface FragmentFocusProps {
  id: string;
  /** The whole record, for traces. Already in memory; this never queries for it. */
  corpus: Fragment[];
  materials?: Materials;
  onLeave: () => void;
  onChanged: () => void;
  onOpen: (f: Fragment) => void;
  /** Keep present / release, for any moment on this surface. */
  onHold: (f: Fragment) => void;
  onRelease: (f: Fragment) => void;
  heldIds: Set<string>;
  /** Open with the continuation field focused — arriving via `continue` rather than a click. */
  startContinuing?: boolean;
  /** Open with this moment already being corrected, for ⋯ → correct. */
  startCorrecting?: boolean;
  /** Removal goes through the shell so undo can live in the quiet line. */
  onRemove?: (f: Fragment) => void | Promise<void>;
}

export function FragmentFocus({
  id,
  corpus,
  materials,
  onLeave,
  onChanged,
  onOpen,
  onHold,
  onRelease,
  heldIds,
  startContinuing = false,
  startCorrecting = false,
  onRemove,
}: FragmentFocusProps) {
  const now = useNow();
  const [line, setLine] = useState<LineMoment[]>([]);
  /** Earlier wordings, by moment, so "wording corrected · show" can show one. */
  const [earlier, setEarlier] = useState<Map<string, string>>(() => new Map());
  const [showEarlier, setShowEarlier] = useState<Set<string>>(() => new Set());
  const [unfolded, setUnfolded] = useState(false);
  const [hovered, setHovered] = useState<string | null>(null);
  const [correctingId, setCorrectingId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [continueDraft, setContinueDraft] = useState("");
  const [rejected, setRejected] = useState<Set<string>>(() => new Set());
  const [playing, setPlaying] = useState<string | null>(null);
  /** Same-source and meaning, folded in after the lexical pass so opening never waits. */
  const [assembled, setAssembled] = useState<Trace[] | null>(null);

  const correctRef = useRef<HTMLTextAreaElement>(null);
  const continueRef = useRef<HTMLTextAreaElement>(null);

  const load = useCallback(async () => {
    const moments = await api.lineFor(id);
    setLine(moments);
    // Only the moments that actually claim a correction are asked for their history.
    const withHistory = moments.filter((m) => m.fragment.correctionCount > 0);
    const out = new Map<string, string>();
    for (const m of withHistory) {
      try {
        const revisions = await api.fragmentHistory(m.fragment.id);
        const previous = revisions[revisions.length - 2];
        if (previous) out.set(m.fragment.id, previous.body);
      } catch {
        // A history that cannot be read simply offers nothing to show.
      }
    }
    setEarlier(out);
  }, [id]);

  useEffect(() => {
    void load();
  }, [load]);

  // Arriving via `continue`: the field is the reason you came, so it gets the caret. Not
  // when you arrived by clicking the fragment — then you came to read it.
  useEffect(() => {
    if (startContinuing) continueRef.current?.focus({ preventScroll: true });
  }, [startContinuing, line.length]);

  useEffect(() => {
    if (!startCorrecting || line.length === 0 || correctingId) return;
    const target = line.find((m) => m.fragment.id === id) ?? line[line.length - 1];
    setCorrectingId(target.fragment.id);
    setDraft(target.fragment.body);
  }, [startCorrecting, line, id, correctingId]);

  useEffect(() => {
    if (!correctingId) return;
    const el = correctRef.current;
    if (!el) return;
    el.focus({ preventScroll: true });
    el.setSelectionRange(el.value.length, el.value.length);
  }, [correctingId]);

  // Both fields grow with their text rather than scrolling inside a box.
  useEffect(() => {
    for (const el of [correctRef.current, continueRef.current]) {
      if (!el) continue;
      el.style.height = "auto";
      el.style.height = `${el.scrollHeight}px`;
    }
  }, [draft, continueDraft, correctingId]);

  /**
   * `esc` inside a correction closes the correction; the surface's own `esc` is handled
   * once, globally, in the shell. Here it only has to stop the global one from firing at
   * the same time, which would close two levels at once.
   */
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key !== "Escape" || !correctingId) return;
      e.preventDefault();
      e.stopPropagation();
      setCorrectingId(null);
      setDraft("");
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [correctingId]);

  async function saveCorrection() {
    if (!correctingId) return;
    const body = draft.trim();
    if (body) await api.correctFragment(correctingId, body);
    setCorrectingId(null);
    setDraft("");
    await load();
    onChanged();
  }

  async function saveContinuation() {
    const body = continueDraft.trim();
    if (!body) return;
    const last = line[line.length - 1];
    if (!last) return;
    setContinueDraft("");
    await api.continueFragment(last.fragment.id, body);
    await load();
    onChanged();
  }

  const focused = line.find((m) => m.fragment.id === id)?.fragment;
  const isLine = line.length > 1;

  const lexical = useMemo(
    () =>
      line.length === 0
        ? []
        : tracesFor(
            line.map((m) => m.fragment),
            corpus,
            rejected,
          ),
    [line, corpus, rejected],
  );

  useEffect(() => {
    setAssembled(null);
    if (line.length === 0) return;
    let stale = false;
    const members = line.map((m) => m.fragment);
    void (async () => {
      const sourceHits: Fragment[] = [];
      for (const m of members) {
        try {
          const rows = await api.sameSourceEncounters(m.id);
          for (const [relatedId] of rows) {
            const found = corpus.find((c) => c.id === relatedId);
            if (found) sourceHits.push(found);
          }
        } catch {
          // A source list that cannot be read simply offers nothing extra.
        }
      }
      let meaningHits: Fragment[] = [];
      try {
        const origin = members.find((m) => m.id === id) ?? members[0];
        const guesses = await api.findByMeaning(
          origin.body,
          members.map((m) => m.id),
          5,
        );
        meaningHits = guesses.map((g) => g.fragment);
      } catch {
        // Meaning that cannot run is not a reason to withhold the lexical traces.
      }
      if (!stale) {
        setAssembled(assembleTraces(members, corpus, rejected, sourceHits, meaningHits));
      }
    })();
    return () => {
      stale = true;
    };
  }, [line, corpus, rejected, id]);

  const traces = assembled ?? lexical;

  // A line long enough to fold does so in the middle: the beginning and the end are what
  // locate it, and the middle is what you unfold when you want to read it.
  const foldable = line.length > FOLD_ABOVE && !unfolded;
  const foldedMoments = foldable ? line.slice(KEEP_HEAD, line.length - KEEP_TAIL) : [];

  const head = isLine ? headLine(line, now) : focused ? `a fragment · ${fullDateLabel(new Date(focused.capturedAt))}` : "";

  return (
    <div>
      <div
        style={{
          ...metaStyle(),
          display: "flex",
          justifyContent: "space-between",
        }}
      >
        <Verb  onClick={onLeave} style={{ cursor: "pointer" }}>
          ‹ back to the edge · esc
        </Verb>
        <span>{head}</span>
      </div>

      <div
        style={{
          marginTop: "34px",
          position: "relative",
          paddingLeft: "var(--indent)",
          display: "flex",
          flexDirection: "column",
          gap: "var(--gap-line-moment)",
        }}
      >
        {/*
          The rail only exists for a line. One moment has nothing to be strung along, and
          drawing a spine beside it would claim a continuity that is not there.
        */}
        <span
          aria-hidden="true"
          style={{
            position: "absolute",
            left: "var(--gutter-width)",
            top: 14,
            bottom: 14,
            width: "2px",
            background: "var(--rule)",
            display: isLine ? "block" : "none",
          }}
        />

        {(foldable ? line.slice(0, KEEP_HEAD) : line).map((m) => renderMoment(m))}

        {foldable ? (
          <div style={{ ...metaStyle(), position: "relative" }}>
            <span
              aria-hidden="true"
              style={{
                position: "absolute",
                left: "-25px",
                top: "6px",
                width: "4px",
                height: "4px",
                background: "var(--meta)",
              }}
            />
            {foldedMoments.length} moments folded ·{" "}
            {foldedMoments
              .map((m) => monthLabel(new Date(m.fragment.capturedAt), now, true))
              .filter((v, i, a) => a.indexOf(v) === i)
              .join(", ")}{" "}
            ·{" "}
            <Verb
              
              onClick={() => setUnfolded(true)}
              style={{ color: "var(--ink-verb)", cursor: "pointer" }}
            >
              unfold
            </Verb>
          </div>
        ) : null}

        {foldable ? line.slice(line.length - KEEP_TAIL).map((m) => renderMoment(m)) : null}

        {/*
          Where the next moment would land. An open square rather than a filled dot: it is a
          place, not a thing — and the field says exactly what pressing return will do.
        */}
        <div
          style={{
            position: "relative",
            display: "flex",
            alignItems: "flex-start",
            gap: "14px",
            minHeight: "36px",
          }}
        >
          <span
            aria-hidden="true"
            style={{
              position: "absolute",
              left: "-27px",
              top: "12px",
              width: "8px",
              height: "8px",
              border: "1.5px solid var(--meta)",
              boxSizing: "border-box",
            }}
          />
          <textarea
            ref={continueRef}
            rows={1}
            value={continueDraft}
            onChange={(e) => setContinueDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                void saveContinuation();
              }
            }}
            placeholder="continue — a new moment, dated now"
            aria-label="Continue this line"
            spellCheck={false}
            style={{
              width: "100%",
              resize: "none",
              border: "none",
              outline: "none",
              background: "transparent",
              color: "var(--ink)",
              font: "inherit",
              fontSize: "var(--size-moment)",
              lineHeight: 1.5,
              fontVariationSettings: "'wdth' 94",
              caretColor: "var(--ink)",
              padding: 0,
              overflow: "hidden",
            }}
          />
        </div>
      </div>

      {traces.length > 0 ? (
        <div
          style={{
            marginTop: "44px",
            paddingLeft: "var(--indent)",
            display: "flex",
            flexDirection: "column",
            gap: "16px",
            fontSize: "var(--size-evidence)",
            lineHeight: 1.35,
            fontVariationSettings: "'wdth' 90",
            color: "var(--ink-far)",
          }}
        >
          <div
            style={{
              ...metaStyle("var(--size-meta-sm)"),
              letterSpacing: "var(--track-tier-label)",
              textTransform: "uppercase",
            }}
          >
            {tracesHeading(traces)}
          </div>
          {traces.map((t) => {
            const at = new Date(t.fragment.capturedAt);
            const when =
              now.getTime() - at.getTime() < 7 * 86_400_000
                ? dayLabel(at, now)
                : monthLabel(at, now);
            return (
              <div key={t.fragment.id} style={{ display: "flex", gap: "22px" }}>
                <span
                  style={{
                    width: "110px",
                    flex: "none",
                    color: "var(--meta)",
                    fontStyle: t.kind === "guess" ? "italic" : "normal",
                  }}
                >
                  {t.kind === "seen" ? "same words" : "a guess"}
                </span>
                <div
                  style={{
                    fontStyle: t.kind === "guess" ? "italic" : "normal",
                    color: t.kind === "guess" ? "var(--ink-dim)" : "var(--ink-far)",
                  }}
                >
                  <span
                    onClick={() => onOpen(t.fragment)}
                    style={{ cursor: "pointer" }}
                  >
                    <Marked
                      text={
                        t.phrase
                          ? snippetAround(t.fragment.body.replace(/\n+/g, " "), t.phrase)
                          : t.fragment.body.split("\n")[0].slice(0, 90)
                      }
                      mark={t.phrase}
                      wash="var(--mark-trace)"
                    />
                  </span>
                  <span style={{ color: "var(--meta)", fontStyle: "normal" }}> · {when}</span>
                  {t.kind === "guess" ? (
                    <>
                      <span style={{ color: "var(--meta)", fontStyle: "normal" }}>
                        {" "}
                        · {t.why} ·{" "}
                      </span>
                      <Verb
                        
                        onClick={() => void acceptTrace(t.fragment)}
                        style={{ color: "var(--ink-verb)", fontStyle: "normal", cursor: "pointer" }}
                      >
                        yes
                      </Verb>
                      <span style={{ color: "var(--meta)", fontStyle: "normal" }}> · </span>
                      <Verb
                        
                        onClick={() => rejectTrace(t.fragment)}
                        style={{ color: "var(--ink-verb)", fontStyle: "normal", cursor: "pointer" }}
                      >
                        not this
                      </Verb>
                    </>
                  ) : null}
                </div>
              </div>
            );
          })}
        </div>
      ) : null}
    </div>
  );

  /** "yes, that belongs here" — joins the trace into this line. */
  async function acceptTrace(f: Fragment) {
    if (!focused) return;
    await api.linkContinuation(f.id, focused.id);
    await load();
    onChanged();
  }

  /** "not this" is remembered, because being offered the same wrong guess twice is worse. */
  function rejectTrace(f: Fragment) {
    setRejected((s) => new Set(s).add(f.id));
    if (focused) void api.rejectGuess(focused.id, f.id).catch(() => {});
  }

  function renderMoment(m: LineMoment) {
    const f = m.fragment;
    const isFocused = f.id === id;
    const isCorrecting = correctingId === f.id;
    const encounter = materials?.encounters.get(f.id);
    const voice = materials?.voices.get(f.id);
    const showVerbs = hovered === f.id && !isCorrecting;
    const previous = earlier.get(f.id);
    const at = new Date(f.capturedAt);

    return (
      <div
        key={f.id}
        onMouseEnter={() => setHovered(f.id)}
        onMouseLeave={() => setHovered((h) => (h === f.id ? null : h))}
        style={{ position: "relative" }}
      >
        <span
          aria-hidden="true"
          style={{
            position: "absolute",
            left: "-27px",
            top: "8px",
            width: "8px",
            height: "8px",
            // Ink for the moment you came for, meta for the rest of the line.
            background: isFocused ? "var(--ink)" : "var(--meta)",
          }}
        />

        <div style={{ ...metaStyle(), marginBottom: "6px", display: "flex", gap: "14px" }}>
          <span>
            {dayLabel(at, now) === "today" ? "today" : fullDateLabel(at)} · {clockLabel(at)} ·{" "}
            {sourceOf(f, Boolean(voice), Boolean(encounter))}
            {voice && f.correctionCount > 0
              ? /*
                  Only a transcript says this, and it is not decoration: a transcript is a
                  DERIVED representation of audio that is still there, so "which version of
                  the derivation am I reading" is a real question about the material.
                  Ordinary text repaired once does not carry a permanent marker.
                */
                ` · transcript corrected ${f.correctionCount === 1 ? "once" : `${f.correctionCount} times`}`
              : ""}
          </span>

          {previous ? (
            <Verb
              quiet
              onClick={() =>
                setShowEarlier((s) => {
                  const next = new Set(s);
                  if (next.has(f.id)) next.delete(f.id);
                  else next.add(f.id);
                  return next;
                })
              }
              style={{ color: "var(--faint)", cursor: "pointer" }}
            >
              wording corrected · {showEarlier.has(f.id) ? "hide" : "show"}
            </Verb>
          ) : null}

          {showVerbs ? (
            <span
              style={{
                marginLeft: "auto",
                display: "flex",
                gap: "16px",
                color: "var(--ink-verb)",
              }}
            >
              <Verb
                bright
                onClick={() => {
                  setCorrectingId(f.id);
                  setDraft(f.body);
                }}
                style={{ cursor: "pointer" }}
              >
                correct
              </Verb>
              <Verb
                bright
                onClick={() => (heldIds.has(f.id) ? onRelease(f) : onHold(f))}
                style={{ cursor: "pointer" }}
              >
                {heldIds.has(f.id) ? "release" : "hold"}
              </Verb>
              <Verb
                bright
                onClick={async () => {
                  if (onRemove) await onRemove(f);
                  else await api.removeFragment(f.id);
                  if (line.length <= 1) onLeave();
                  else await load();
                  onChanged();
                }}
                style={{ cursor: "pointer" }}
              >
                remove
              </Verb>
            </span>
          ) : null}
        </div>

        {isCorrecting ? (
          <>
            <textarea
              ref={correctRef}
              rows={1}
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  void saveCorrection();
                }
              }}
              spellCheck={false}
              aria-label="Correct the wording"
              style={{
                width: "100%",
                resize: "none",
                border: "none",
                outline: "none",
                background: "transparent",
                color: "var(--ink)",
                font: "inherit",
                fontSize: "var(--size-d0)",
                lineHeight: 1.24,
                letterSpacing: "var(--track-d0)",
                boxShadow: "inset 0 -2px var(--correction-rule)",
                paddingBottom: "4px",
                overflow: "hidden",
              }}
            />
            {/* The operation explains itself where it happens, not in a menu. */}
            <div style={{ ...metaStyle(), marginTop: "10px", display: "flex", gap: "18px" }}>
              <span>
                {voice
                  ? "corrects the transcript only — the recording and its date are untouched"
                  : "changes the wording only — the moment keeps its date and the earlier wording"}
              </span>
              <Verb
                
                onClick={() => void saveCorrection()}
                style={{ marginLeft: "auto", color: "var(--ink)", cursor: "pointer" }}
              >
                ⏎ save
              </Verb>
              <Verb
                
                onClick={() => {
                  setCorrectingId(null);
                  setDraft("");
                }}
                style={{ cursor: "pointer" }}
              >
                esc cancel
              </Verb>
            </div>
          </>
        ) : (
          <>
            {encounter?.selectedText ? (
              <div
                style={{
                  color: "var(--ink-far)",
                  paddingLeft: "18px",
                  borderLeft: "2px solid var(--rule)",
                  fontSize: "20px",
                  lineHeight: 1.3,
                  marginBottom: "10px",
                  textWrap: "pretty",
                }}
              >
                “{encounter.selectedText}”
              </div>
            ) : null}

            <div
              style={
                isFocused
                  ? {
                      fontSize: "var(--size-d0)",
                      lineHeight: 1.24,
                      letterSpacing: "var(--track-d0)",
                      color: voice ? "var(--ink-far)" : "var(--ink)",
                      fontStyle: voice ? "italic" : "normal",
                      textWrap: "pretty",
                      overflowWrap: "anywhere",
                    }
                  : {
                      fontSize: "var(--size-moment)",
                      lineHeight: 1.28,
                      color: voice ? "var(--ink-far)" : "var(--ink-near)",
                      fontStyle: voice ? "italic" : "normal",
                      fontVariationSettings: "'wdth' 94",
                      textWrap: "pretty",
                      overflowWrap: "anywhere",
                    }
              }
            >
              {voice ? (
                <button
                  type="button"
                  onClick={() => setPlaying((p) => (p === f.id ? null : f.id))}
                  disabled={voice.audioMissing}
                  style={{
                    display: "inline-block",
                    fontSize: "var(--size-voice-chip)",
                    color: voice.audioMissing ? "var(--meta)" : "var(--ink-verb)",
                    border: "1px solid var(--rule)",
                    borderStyle: voice.audioMissing ? "dashed" : "solid",
                    padding: "3px 10px 3px 8px",
                    verticalAlign: "4px",
                    marginRight: "12px",
                    fontStyle: "normal",
                    background: "none",
                    cursor: voice.audioMissing ? "default" : "pointer",
                    fontVariationSettings: "'wdth' 90",
                  }}
                >
                  {voice.audioMissing ? "⌀" : playing === f.id ? "■" : "▶"}{" "}
                  {durationLabel(voice.durationMs / 1000)}
                </button>
              ) : null}
              {f.body.split(/\n\n+/).map((para, i) =>
                i === 0 ? (
                  <span key={i}>{para}</span>
                ) : (
                  <div key={i} style={{ marginTop: "12px" }}>
                    {para}
                  </div>
                ),
              )}
            </div>

            {encounter ? (
              <div
                style={{
                  marginTop: "6px",
                  fontSize: "14px",
                  color: "var(--meta)",
                  fontVariationSettings: "'wdth' 90",
                }}
              >
                {encounter.domain ?? "a link"}
                {encounter.title ? ` · ${encounter.title}` : ""}
              </div>
            ) : null}

            {previous && showEarlier.has(f.id) ? (
              <div
                style={{
                  marginTop: "10px",
                  fontSize: "var(--size-evidence)",
                  color: "var(--meta)",
                  fontStyle: "italic",
                  lineHeight: 1.35,
                  fontVariationSettings: "'wdth' 92",
                }}
              >
                earlier wording · {previous}
              </div>
            ) : null}
          </>
        )}
      </div>
    );
  }
}

/** "a line · 7 moments · 2019 → today" */
function headLine(line: LineMoment[], now: Date): string {
  const first = new Date(line[0].fragment.capturedAt);
  const last = new Date(line[line.length - 1].fragment.capturedAt);
  const end = dayLabel(last, now) === "today" ? "today" : last.getFullYear();
  return `a line · ${line.length} moments · ${first.getFullYear()} → ${end}`;
}

/**
 * How a moment came to exist, said only where it is actually known. Material carried over
 * from v1 has no recorded capture method, so it says nothing rather than asserting one.
 */
function sourceOf(f: Fragment, isVoice: boolean, isEncounter: boolean): string {
  if (isVoice) return "voice";
  if (isEncounter) return f.captureOrigin === "share" ? "shared" : "typed";
  if (f.captureMethod === "imported") return "carried over";
  if (f.captureOrigin === "menubar") return "menu bar";
  return f.captureMethod;
}
