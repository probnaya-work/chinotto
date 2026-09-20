/**
 * A Return, sitting in Now.
 *
 * Four lines, as the prototype draws them: where it is from, the material (with the shared
 * run marked), one because-sentence quoting the causing act, and the verbs. The spine is
 * ink rather than the rule grey used for held material: that single difference is what
 * says "this came back" instead of "this is being kept".
 *
 * Today does not shrink to make room. A Return is one thing in Now, never a section, and
 * `let go` records an outcome rather than deleting anything.
 */

import type { Fragment } from "../../lib/recordApi";
import { metaStyle } from "../../design/tiers";
import { useNow } from "./useNow";
import { Marked } from "./Marked";
import { snippetAround } from "./words";
import { agoLabel, clockLabel, dayLabel, fullDateLabel, monthLabel } from "./format";

export interface ReturnEvidence {
  kind: string;
  detail: string;
  occurredAt: string | null;
  relatedId?: string | null;
}

export interface ReturnValue {
  id: number;
  fragment: Fragment;
  reason: string;
  evidence: ReturnEvidence[];
  because?: Fragment | null;
}

export interface ReturnBlockProps {
  value: ReturnValue;
  domain?: string | null;
  leaving?: boolean;
  onOpen: (f: Fragment) => void;
  onContinue: (f: Fragment) => void;
  onHold: (f: Fragment) => void;
  onLetGo: (value: ReturnValue) => void;
}

export function phraseOn(evidence: ReturnEvidence[], related: boolean): string | null {
  const hit = evidence.find(
    (e) => e.kind === "shared_phrase" && Boolean(e.relatedId) === related,
  );
  return hit?.detail ?? evidence.find((e) => e.kind === "shared_phrase")?.detail ?? null;
}

/**
 * The one because-sentence. Each surviving trigger has its own clause in the prototype's
 * shape; a trigger that cannot fill this in must not have become a Return.
 *
 * Exported because the menu-bar panel draws the same Return at its own scale. A Return that
 * read one way in the window and another way under the glyph would be two Returns, and
 * there is only ever one.
 */
export function becauseSentence(
  r: ReturnValue,
  now: Date,
): { when: string; quote: string | null; mark: string | null } | { opened: string } | { added: string; quote: string; mark: string | null } | null {
  const cause = r.because;
  const causeAt = cause ? new Date(cause.capturedAt) : null;

  if (r.reason === "repeated_language" && cause && causeAt) {
    const mark = phraseOn(r.evidence, true);
    const quote = mark ? snippetAround(cause.body.replace(/\n+/g, " "), mark) : cause.body.split("\n")[0].slice(0, 90);
    return {
      when: `at ${clockLabel(causeAt)} ${dayLabel(causeAt, now)}`,
      quote,
      mark,
    };
  }

  if (r.reason === "same_source" && cause && causeAt) {
    const url = r.evidence.find((e) => e.kind === "url");
    const source = url?.detail ?? "this source";
    return { opened: `you opened ${source} again at ${clockLabel(causeAt)} ${dayLabel(causeAt, now)}` };
  }

  if (r.reason === "line_continued") {
    const cont = r.evidence.find((e) => e.kind === "continuation");
    const at = cont?.occurredAt ? new Date(cont.occurredAt) : causeAt;
    if (!at || !cont?.detail) return null;
    return {
      added: `in ${monthLabel(at, now, true)} you added`,
      quote: cont.detail,
      mark: null,
    };
  }

  return null;
}

export function ReturnBlock({
  value,
  domain,
  leaving = false,
  onOpen,
  onContinue,
  onHold,
  onLetGo,
}: ReturnBlockProps) {
  const now = useNow();
  const { fragment } = value;
  const from = new Date(fragment.capturedAt);
  const phraseOld = phraseOn(value.evidence, false);
  const because = becauseSentence(value, now);

  return (
    <div
      style={{
        marginTop: "16px",
        paddingLeft: "var(--indent)",
        position: "relative",
        display: "flex",
        flexDirection: "column",
        gap: "10px",
        transition: "transform var(--let-go) ease-in, opacity var(--let-go) ease-in",
        transform: leaving ? "translateY(60px)" : "none",
        opacity: leaving ? 0 : 1,
        animation: leaving ? "none" : "chinotto-rise var(--rise) var(--ease)",
      }}
    >
      <span
        aria-hidden="true"
        style={{
          position: "absolute",
          left: "var(--gutter-width)",
          top: "6px",
          bottom: "6px",
          width: "2px",
          background: "var(--rule-present)",
        }}
      />

      <div style={metaStyle()}>
        back from {fullDateLabel(from)} · {agoLabel(from, now)}
      </div>

      <div
        onClick={() => onOpen(fragment)}
        style={{
          fontSize: "var(--size-return)",
          lineHeight: 1.22,
          letterSpacing: "var(--track-return)",
          textWrap: "pretty",
          overflowWrap: "anywhere",
          cursor: "pointer",
        }}
      >
        <Marked text={fragment.body} mark={phraseOld} />
        {domain ? (
          <span
            style={{
              display: "block",
              marginTop: "6px",
              fontSize: "14px",
              color: "var(--meta)",
              letterSpacing: 0,
              fontVariationSettings: "'wdth' 90",
            }}
          >
            {domain}
          </span>
        ) : null}
      </div>

      {because && "when" in because ? (
        <div
          style={{
            fontSize: "var(--size-evidence)",
            color: "var(--ink-far)",
            lineHeight: 1.35,
            fontVariationSettings: "'wdth' 92",
            textWrap: "pretty",
          }}
        >
          because {because.when} you wrote “
          <Marked text={because.quote ?? ""} mark={because.mark} />”
        </div>
      ) : because && "opened" in because ? (
        <div
          style={{
            fontSize: "var(--size-evidence)",
            color: "var(--ink-far)",
            lineHeight: 1.35,
            fontVariationSettings: "'wdth' 92",
            textWrap: "pretty",
          }}
        >
          because {because.opened}
        </div>
      ) : because && "added" in because ? (
        <div
          style={{
            fontSize: "var(--size-evidence)",
            color: "var(--ink-far)",
            lineHeight: 1.35,
            fontVariationSettings: "'wdth' 92",
            textWrap: "pretty",
          }}
        >
          because {because.added} “{because.quote}”
        </div>
      ) : null}

      <div style={{ ...metaStyle("var(--size-meta-lg)"), display: "flex", gap: "24px", marginTop: "6px" }}>
        <Verb label="continue" emphasis onClick={() => onContinue(fragment)} />
        <Verb label="open the line" onClick={() => onOpen(fragment)} />
        <Verb label="hold" onClick={() => onHold(fragment)} />
        <span style={{ marginLeft: "auto" }}>
          <Verb label="let go ↓" onClick={() => onLetGo(value)} />
        </span>
      </div>
    </div>
  );
}

function Verb({
  label,
  onClick,
  emphasis = false,
}: {
  label: string;
  onClick: () => void;
  emphasis?: boolean;
}) {
  return (
    <button
      type="button"
      className={"chinotto-verb" + (emphasis ? "" : " chinotto-verb--quiet")}
      onClick={onClick}
      style={{
        background: "none",
        border: "none",
        padding: 0,
        font: "inherit",
        // `continue` is the Return's one full verb; `open the line`, `hold` and `let go ↓`
        // are the quiet rank beside it.
        color: emphasis ? "var(--agency)" : "var(--agency-quiet)",
        cursor: "pointer",
      }}
    >
      {label}
    </button>
  );
}
