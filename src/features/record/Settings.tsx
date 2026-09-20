/**
 * Settings — the instrument's own register.
 *
 * It replaces the column; it never wraps it. `esc` goes back to the edge.
 *
 * Utility surfaces may be conventional, but they use the record's type, colour, spacing and
 * verbs. Text verbs, not switches. No cards, no toggles, no modal dialogs — everything that
 * needs explaining expands in place, including the two-step account deletion, because a
 * dialog would be the only object in the product that stops you doing anything else.
 */

import { useEffect, useState, type ReactNode } from "react";
import { metaStyle } from "../../design/tiers";
import { Mark } from "./Mark";
import {
  ZOOM_STEP,
  textScaleLabel,
  type Appearance,
} from "../../lib/appearance";
import { Verb } from "./Verb";

export type MicrophoneState = "granted" | "ask" | "denied";

export interface SettingsProps {
  version: string;

  /** One status sentence, and the verb that opens the sync surface. */
  syncLine: string;
  syncVerb: "set up" | "manage" | "fix";
  onOpenSync: () => void;
  /** The account section exists only once sync knows who you are. */
  hasAccount: boolean;

  microphone: MicrophoneState;
  onOpenSystemSettings: () => void;
  onTryMenuBar: () => void;

  appearance: Appearance;
  onAppearance: (a: Appearance) => void;
  liftContrast: boolean;
  onLiftContrast: (on: boolean) => void;

  textScale: number;
  onTextScale: (percent: number) => void;

  iconVariant: "dark" | "light";
  onIconVariant: (v: "dark" | "light") => void;

  onExport: () => void;
  exportNote: string;
  backupLine: string;
  onBackUpNow: () => void;

  analyticsOn: boolean;
  onAnalytics: (on: boolean) => void;

  updateLine: string;
  updateVerb: string | null;
  onUpdate: () => void;

  onDeleteAccount: () => Promise<void> | void;
  onLeave: () => void;
}

export function Settings(props: SettingsProps) {
  const [privacyOpen, setPrivacyOpen] = useState(false);
  const [del, setDel] = useState<"idle" | "armed" | "done">("idle");

  // Leaving and coming back must not leave the confirmation armed: a destructive step that
  // survives navigation is a destructive step you can trigger by accident.
  useEffect(() => () => setDel("idle"), []);

  const micLine =
    props.microphone === "denied"
      ? "not allowed by the mac."
      : props.microphone === "ask"
        ? "the mac hasn’t been asked yet · it asks the first time you hold space."
        : "allowed.";

  return (
    <div>
      <div style={{ ...metaStyle(), display: "flex", justifyContent: "space-between" }}>
        <Verb  onClick={props.onLeave} style={{ cursor: "pointer" }}>
          ‹ back to the edge · esc
        </Verb>
        <span>settings · this mac</span>
      </div>

      <div
        style={{
          marginTop: "34px",
          paddingLeft: "var(--indent)",
          maxWidth: "var(--settings-measure)",
          display: "flex",
          flexDirection: "column",
          gap: "var(--utility-section-gap)",
          fontSize: "var(--size-utility)",
          lineHeight: 1.45,
          color: "var(--ink-near)",
          fontVariationSettings: "'wdth' 94",
          textWrap: "pretty",
        }}
      >
        <Row label="sync">
          <div>
            {props.syncLine}{" "}
            <Verb ink onClick={props.onOpenSync}>
              {props.syncVerb} ›
            </Verb>
          </div>
        </Row>

        <Row label="from anywhere">
          <Stack>
            <div>
              menu bar · <Key>⌘⇧K</Key> opens a small field over whatever you’re doing.{" "}
              <Verb onClick={props.onTryMenuBar}>try it</Verb>
            </div>
            <div>
              voice · hold <Key>space</Key> at the edge, or <Key>⌥space</Key> anywhere on the
              mac.
            </div>
            <div style={{ color: "var(--ink-far)" }}>
              microphone · {micLine}{" "}
              {props.microphone === "denied" ? (
                <Verb ink onClick={props.onOpenSystemSettings}>
                  open system settings ›
                </Verb>
              ) : null}
            </div>
          </Stack>
        </Row>

        <Row label="appearance">
          <Stack>
            <div style={{ display: "flex", gap: "22px", alignItems: "baseline" }}>
              {(["system", "light", "dark"] as const).map((id) => (
                <Verb
                  key={id}
                  onClick={() => props.onAppearance(id)}
                  aria-label={`appearance: ${id}`}
                  style={{
                    fontSize: "var(--size-utility)",
                    color: props.appearance === id ? "var(--ink)" : "var(--meta)",
                    boxShadow: props.appearance === id ? "inset 0 -2px var(--ink)" : "none",
                    paddingBottom: "3px",
                  }}
                >
                  {id}
                </Verb>
              ))}
            </div>
            <Secondary>
              {props.appearance === "system"
                ? "follows the mac. dark is the record’s home; light is a full second appearance, not a washed-out dark one."
                : props.appearance === "light"
                  ? "paper field, ink words. the same distances, read the other way round."
                  : "the ink field, always."}
            </Secondary>
            <div style={{ color: "var(--ink-far)" }}>
              lift contrast in bright light · <Key>{props.liftContrast ? "on" : "off"}</Key> ·{" "}
              <Verb onClick={() => props.onLiftContrast(!props.liftContrast)}>
                {props.liftContrast ? "turn off" : "turn on"}
              </Verb>
            </div>
          </Stack>
        </Row>

        <Row label="text">
          <div style={{ display: "flex", gap: "22px", alignItems: "baseline" }}>
            <Verb ink onClick={() => props.onTextScale(props.textScale - ZOOM_STEP)}>
              smaller ⌘−
            </Verb>
            <Verb ink onClick={() => props.onTextScale(props.textScale + ZOOM_STEP)}>
              larger ⌘+
            </Verb>
            <span style={{ color: "var(--meta)" }}>{textScaleLabel(props.textScale)}</span>
          </div>
        </Row>

        <Row label="dock icon">
          <Stack gap="12px">
            <div style={{ display: "flex", gap: "20px", alignItems: "flex-end" }}>
              {(["dark", "light"] as const).map((id) => (
                <Verb
                  key={id}
                  onClick={() => props.onIconVariant(id)}
                  aria-label={`dock icon: ${id}`}
                  style={{
                    display: "flex",
                    flexDirection: "column",
                    gap: "9px",
                    alignItems: "center",
                  }}
                >
                  <span
                    style={{
                      width: 56,
                      height: 56,
                      borderRadius: 13,
                      background: id === "dark" ? "#141416" : "#f2f1ec",
                      border:
                        props.iconVariant === id
                          ? "1px solid var(--ink)"
                          : `1px solid ${id === "dark" ? "#2a2a2e" : "#c9c9c6"}`,
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center",
                      boxSizing: "border-box",
                    }}
                  >
                    <Mark size={35} ink={id === "dark" ? "#e6e6e3" : "#1b1b1d"} appIcon />
                  </span>
                  <span
                    style={{
                      ...metaStyle(),
                      color: props.iconVariant === id ? "var(--ink)" : "var(--meta)",
                    }}
                  >
                    {id}
                  </span>
                </Verb>
              ))}

              {/*
                The two rungs the system draws for itself. Shown at their real size, because
                the only way to get the ladder wrong is to pick the wrong rung and a preview
                that scales one drawing would hide exactly that.
              */}
              <div
                style={{
                  display: "flex",
                  gap: "14px",
                  alignItems: "center",
                  paddingBottom: "26px",
                  marginLeft: "8px",
                }}
              >
                <span
                  style={{
                    width: 32,
                    height: 32,
                    borderRadius: 7,
                    background: "#141416",
                    border: "1px solid #2a2a2e",
                    boxSizing: "border-box",
                    display: "inline-flex",
                    alignItems: "center",
                    justifyContent: "center",
                  }}
                >
                  <Mark size={20} ink="#e6e6e3" />
                </span>
                <span
                  style={{
                    width: 16,
                    height: 16,
                    borderRadius: 4,
                    background: "#141416",
                    border: "0.5px solid #3a3a40",
                    boxSizing: "border-box",
                    display: "inline-flex",
                    alignItems: "center",
                    justifyContent: "center",
                  }}
                >
                  <Mark size={11} ink="#e6e6e3" />
                </span>
                <span style={metaStyle("var(--size-meta-lg)")}>
                  as finder and spotlight draw it
                </span>
              </div>
            </div>
            <Secondary>
              the menu bar keeps its own glyph — one dot, black on transparent, tinted by the
              system.
            </Secondary>
          </Stack>
        </Row>

        <Row label="the record">
          <Stack>
            <div>
              everything lives in one file on this mac.{" "}
              <Verb ink onClick={props.onExport}>
                export it ›
              </Verb>{" "}
              <span style={{ color: "var(--meta)" }}>
                plain text and audio, zipped. {props.exportNote}
              </span>
            </div>
            <div>
              a backup is made each time chinotto opens · last {props.backupLine} ·{" "}
              <Verb ink onClick={props.onBackUpNow}>
                back up now
              </Verb>
            </div>
          </Stack>
        </Row>

        <Row label="privacy">
          <Stack>
            <div>
              the words never leave this mac unless sync is on, and then only to your own
              devices.
            </div>
            <div>
              anonymous usage · <Key>{props.analyticsOn ? "on" : "off"}</Key> ·{" "}
              <Verb onClick={() => props.onAnalytics(!props.analyticsOn)}>
                {props.analyticsOn ? "turn off" : "turn on"}
              </Verb>{" "}
              · <Verb onClick={() => setPrivacyOpen((o) => !o)}>
                {privacyOpen ? "hide" : "what is sent?"}
              </Verb>
            </div>
            {privacyOpen ? (
              <Secondary>
                only event names and counts — “fragment left”, “find used · 12 results”. never
                the words, never what you searched for, never anything that identifies you.
                off unless you turn it on.
              </Secondary>
            ) : null}
          </Stack>
        </Row>

        <Row label="keys">
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "130px minmax(0, 1fr)",
              gap: "7px 18px",
              fontSize: "var(--size-utility-2)",
              lineHeight: 1.4,
              fontVariationSettings: "'wdth' 92",
            }}
          >
            {KEYS.map(([key, what]) => (
              <Fragment2 key={key}>
                <Key>{key}</Key>
                <span>{what}</span>
              </Fragment2>
            ))}
          </div>
        </Row>

        {props.hasAccount ? (
          <Row label="account">
            <Stack>
              <div>apple id · the only thing sync knows about you.</div>
              {del === "idle" ? (
                <div>
                  <Verb onClick={() => setDel("armed")}>delete the cloud account ›</Verb>
                </div>
              ) : null}
              {del === "armed" ? (
                <div
                  style={{
                    fontSize: "var(--size-utility-2)",
                    lineHeight: 1.45,
                    color: "var(--ink-far)",
                    borderLeft: "2px solid var(--faint)",
                    paddingLeft: "18px",
                    fontVariationSettings: "'wdth' 92",
                    display: "flex",
                    flexDirection: "column",
                    gap: "8px",
                  }}
                >
                  <span>
                    the copy of the record in the cloud goes, for good. the record on this mac
                    and on your phone stays exactly as it is — they just stop meeting. apple
                    will ask you to sign in once more.
                  </span>
                  <span style={{ color: "var(--meta)" }}>
                    a subscription is apple’s: cancel it in the app store first if you don’t
                    want it to renew.
                  </span>
                  <span>
                    <Verb
                      ink
                      onClick={async () => {
                        await props.onDeleteAccount();
                        setDel("done");
                      }}
                    >
                      delete for good
                    </Verb>{" "}
                    ·{" "}
                    <Verb onClick={() => setDel("idle")} tone="var(--ink-verb)">
                      keep it
                    </Verb>
                  </span>
                </div>
              ) : null}
              {del === "done" ? (
                <div style={{ color: "var(--ink-far)" }}>
                  the cloud account is gone. the record here is untouched.
                </div>
              ) : null}
            </Stack>
          </Row>
        ) : null}

        <Row label="about">
          <Stack gap="14px">
            <div style={{ display: "flex", alignItems: "center", gap: "13px" }}>
              <Mark size={30} />
              <span
                style={{
                  fontSize: "var(--size-wordmark)",
                  letterSpacing: "var(--track-wordmark)",
                  fontVariationSettings: "'wdth' 96",
                  fontWeight: 500,
                  color: "var(--ink)",
                  lineHeight: 1,
                }}
              >
                chinotto
              </span>
            </div>
            <div>
              {props.version} · <span style={{ color: "var(--meta)" }}>{props.updateLine}</span>{" "}
              {props.updateVerb ? (
                <Verb ink onClick={props.onUpdate}>
                  {props.updateVerb} ›
                </Verb>
              ) : null}
            </div>
            <div style={{ color: "var(--meta)" }}>the manifesto · on getchinotto.app</div>
            {/* Secondary maker's mark. PROBNAYA is the laboratory; Chinotto keeps its own identity. */}
            <div style={{ color: "var(--meta)" }}>PROBNAYA · Independent Computational Laboratory</div>
          </Stack>
        </Row>
      </div>
    </div>
  );
}

const KEYS: [string, ReactNode][] = [
  ["⏎", "leave it"],
  ["⇧⏎", "new line"],
  ["esc", "drop it · step back"],
  ["space, held", "speak"],
  ["/", "find"],
  ["a date", "stand there — “march 2024”, “2019”, “today”"],
  ["↓", "into the record · c continue · h hold"],
  ["⌘Z", "bring back what you removed"],
  ["⌘⇧K", "capture from anywhere"],
  ["⌥space", "speak from anywhere"],
  ["⌘+ ⌘−", "text size"],
  ["⌘,", "settings"],
];

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div
      className="chinotto-utility-row"
      style={{
        display: "grid",
        gridTemplateColumns: "var(--utility-label-width) minmax(0, 1fr)",
        gap: "var(--utility-gap)",
      }}
    >
      <div
        style={{
          ...metaStyle("var(--size-meta-sm)"),
          letterSpacing: "var(--track-tier-label)",
          textTransform: "uppercase",
          paddingTop: "6px",
        }}
      >
        {label}
      </div>
      <div>{children}</div>
    </div>
  );
}

function Stack({ children, gap = "10px" }: { children: ReactNode; gap?: string }) {
  return <div style={{ display: "flex", flexDirection: "column", gap }}>{children}</div>;
}

function Secondary({ children }: { children: ReactNode }) {
  return (
    <div
      style={{
        fontSize: "var(--size-utility-2)",
        color: "var(--meta)",
        lineHeight: 1.45,
        fontVariationSettings: "'wdth' 92",
      }}
    >
      {children}
    </div>
  );
}

/** A key, or the current value of a setting: both are things the product states, not verbs. */
function Key({ children }: { children: ReactNode }) {
  return <span style={{ color: "var(--ink)" }}>{children}</span>;
}


/** Keys render as two grid cells, so they need a fragment with a key on it. */
function Fragment2({ children }: { key: string; children: ReactNode }) {
  return <>{children}</>;
}
