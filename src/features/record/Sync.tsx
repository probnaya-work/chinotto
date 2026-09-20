/**
 * Sync — one flow, six states.
 *
 * `esc` goes back to settings, not to the edge: sync is reached through settings and steps
 * back the way it came.
 *
 * The argument the whole surface makes is that sync is never in the way. Capture is local
 * and instant whatever this says; being offline is a fact stated once, not an error; and a
 * moment worded on two devices is not a problem to solve but a question only the person can
 * answer, so both wordings are kept and neither is discarded.
 */

import { useState, type ReactNode } from "react";
import QRCode from "react-qr-code";
import { metaStyle } from "../../design/tiers";
import { clockLabel, dayLabel } from "./format";
import type { SyncDevice } from "../../lib/syncDevices";
import type { WordingConflict } from "../../lib/recordApi";

export type SyncState = "off" | "connecting" | "on" | "error";

export interface SyncProps {
  state: SyncState;
  offline: boolean;
  /** How many local moments have not reached the other devices yet. */
  pending: number;
  /** Null while there is nothing truthful to list. */
  devices: SyncDevice[] | null;
  conflicts: WordingConflict[];
  now: Date;

  /** The phone has reported in, so "continue with apple" can do something. */
  phoneReady: boolean;
  onAlreadyDone: () => void;
  onContinueWithApple: () => void;
  qrUrl: string;

  onRemoveDevice: (id: string) => void;
  onStopSyncing: () => void;
  onResolveConflict: (fragmentId: string, shows: "local" | "remote") => void;
  onOpenFragment: (fragmentId: string) => void;

  /** When the sign-in expired, said in the person's own terms. */
  expiredWhen: string;

  onLeave: () => void;
}

export function Sync(props: SyncProps) {
  const [confirm, setConfirm] = useState<{ kind: "device"; id: string } | { kind: "stop" } | null>(
    null,
  );

  const pendingPhrase =
    props.pending === 1 ? "1 moment is" : `${props.pending} moments are`;

  return (
    <div>
      <div style={{ ...metaStyle(), display: "flex", justifyContent: "space-between" }}>
        <span className="chinotto-verb" onClick={props.onLeave} style={{ cursor: "pointer" }}>
          ‹ settings · esc
        </span>
        <span>sync</span>
      </div>

      <div
        style={{
          marginTop: "34px",
          paddingLeft: "var(--indent)",
          maxWidth: "var(--sync-measure)",
          display: "flex",
          flexDirection: "column",
          gap: "26px",
          fontVariationSettings: "'wdth' 94",
          textWrap: "pretty",
        }}
      >
        {props.state === "off" ? (
          <>
            <Head>the record can follow you to your phone.</Head>
            <Secondary>
              local by default. sync is optional, and never in the way of leaving a fragment.
            </Secondary>
            <div style={{ display: "flex", gap: "36px", alignItems: "flex-start", marginTop: "6px" }}>
              <QrTile url={props.qrUrl} />
              <div
                style={{
                  display: "flex",
                  flexDirection: "column",
                  gap: "16px",
                  fontSize: "var(--size-utility)",
                  lineHeight: 1.4,
                  color: "var(--ink-near)",
                }}
              >
                <div>
                  <Step>1</Step>
                  {props.phoneReady
                    ? "the phone is done."
                    : "open chinotto on your iphone and point it here."}
                </div>
                <div style={{ color: props.phoneReady ? "var(--ink-near)" : "var(--meta)" }}>
                  <Step>2</Step>
                  continue with apple on this mac — same apple id.
                </div>
                <div
                  style={{ display: "flex", gap: "22px", alignItems: "center", marginTop: "6px" }}
                >
                  {/*
                    Inert until the phone has reported in. Signing in first creates an
                    account with nothing to sync to, which looks like it worked and is not
                    recoverable without explaining what happened.
                  */}
                  <span
                    onClick={props.phoneReady ? props.onContinueWithApple : undefined}
                    style={{
                      height: 44,
                      padding: "0 20px",
                      display: "inline-flex",
                      alignItems: "center",
                      fontSize: "var(--size-utility-2)",
                      cursor: props.phoneReady ? "pointer" : "default",
                      background: props.phoneReady ? "var(--ink)" : "transparent",
                      color: props.phoneReady ? "var(--surface)" : "var(--faint)",
                      border: props.phoneReady
                        ? "1px solid var(--ink)"
                        : "1px solid var(--rule)",
                      transition: "all .25s",
                    }}
                  >
                    continue with apple
                  </span>
                  {!props.phoneReady ? (
                    <Verb onClick={props.onAlreadyDone}>already done on the phone?</Verb>
                  ) : null}
                </div>
              </div>
            </div>
          </>
        ) : null}

        {props.state === "connecting" ? (
          <>
            <div style={{ display: "flex", alignItems: "center", gap: "22px" }}>
              <span
                aria-hidden="true"
                style={{ display: "flex", alignItems: "center", gap: "3px", height: "24px" }}
              >
                {["0.9s", "1.1s", "0.7s"].map((d) => (
                  <span
                    key={d}
                    className="chinotto-rec-bar"
                    style={{
                      display: "inline-block",
                      width: "3px",
                      height: "24px",
                      background: "var(--meta)",
                      animation: `chinotto-rec ${d} ease-in-out infinite`,
                    }}
                  />
                ))}
              </span>
              <Head>connecting this mac…</Head>
            </div>
            <Secondary>the phone already has the record. the mac is checking in.</Secondary>
          </>
        ) : null}

        {props.state === "on" ? (
          <>
            <Head>{props.offline ? "sync is on. the mac is offline." : "sync is on."}</Head>
            <Secondary>
              {props.offline
                ? "everything you leave still lands here first."
                : "new moments go across in the background. nothing here waits on it."}
            </Secondary>

            {props.offline && props.pending > 0 ? (
              <div
                style={{
                  fontSize: "var(--size-utility)",
                  lineHeight: 1.4,
                  color: "var(--ink-far)",
                  borderLeft: "2px solid var(--faint)",
                  paddingLeft: "18px",
                }}
              >
                offline · {pendingPhrase} waiting here. they go the moment the mac is back
                online; nothing needs you.
              </div>
            ) : null}

            <div
              style={{
                marginTop: "8px",
                display: "flex",
                flexDirection: "column",
                gap: "14px",
                fontSize: "var(--size-utility)",
                color: "var(--ink-near)",
              }}
            >
              <div
                style={{
                  ...metaStyle("var(--size-meta-sm)"),
                  letterSpacing: "var(--track-tier-label)",
                  textTransform: "uppercase",
                }}
              >
                devices on this record
              </div>

              {props.devices === null ? (
                <Secondary>
                  the device list has not arrived yet. nothing is wrong; it comes with the
                  next check-in.
                </Secondary>
              ) : (
                props.devices.map((d) => (
                  <div key={d.id}>
                    <div style={{ display: "flex", gap: "18px", alignItems: "baseline" }}>
                      <span
                        aria-hidden="true"
                        style={{
                          width: 8,
                          height: 8,
                          background: d.isThisDevice ? "var(--ink)" : "var(--meta)",
                          flex: "none",
                          alignSelf: "center",
                        }}
                      />
                      <span style={{ flex: 1 }}>{d.isThisDevice ? "this mac" : d.name}</span>
                      <span style={metaStyle("var(--size-meta-lg)")}>
                        {lastSeen(d, props.offline, props.now)}
                      </span>
                      {!d.isThisDevice ? (
                        <Verb onClick={() => setConfirm({ kind: "device", id: d.id })}>
                          remove
                        </Verb>
                      ) : null}
                    </div>

                    {confirm?.kind === "device" && confirm.id === d.id ? (
                      <InPlace>
                        remove {d.name} from this record? what is already on the phone stays
                        there; it just stops receiving.{" "}
                        <Verb
                          ink
                          onClick={() => {
                            props.onRemoveDevice(d.id);
                            setConfirm(null);
                          }}
                        >
                          remove
                        </Verb>{" "}
                        · <Verb tone="var(--ink-verb)" onClick={() => setConfirm(null)}>keep</Verb>
                      </InPlace>
                    ) : null}
                  </div>
                ))
              )}

              {props.devices !== null && props.devices.filter((d) => !d.isThisDevice).length === 0 ? (
                <div
                  style={{
                    ...metaStyle("var(--size-utility-2)"),
                    paddingLeft: "26px",
                    fontVariationSettings: "'wdth' 92",
                  }}
                >
                  no other device · open chinotto on a phone and point it at
                  getchinotto.app/sync
                </div>
              ) : null}
            </div>

            <div style={{ marginTop: "10px", ...metaStyle("var(--size-meta-lg)") }}>
              <Verb onClick={() => setConfirm({ kind: "stop" })}>stop syncing on this mac</Verb>
            </div>
            {confirm?.kind === "stop" ? (
              <InPlace indent={false}>
                stop syncing on this mac? the record stays here in full — it just stops
                travelling.{" "}
                <Verb
                  ink
                  onClick={() => {
                    props.onStopSyncing();
                    setConfirm(null);
                  }}
                >
                  stop
                </Verb>{" "}
                ·{" "}
                <Verb tone="var(--ink-verb)" onClick={() => setConfirm(null)}>
                  keep syncing
                </Verb>
              </InPlace>
            ) : null}
          </>
        ) : null}

        {props.state === "error" ? (
          <>
            <Head>sync stopped.</Head>
            <div
              style={{
                fontSize: "var(--size-utility)",
                lineHeight: 1.4,
                color: "var(--ink-near)",
              }}
            >
              this mac’s sign-in expired {props.expiredWhen}. nothing was lost —{" "}
              {pendingPhrase} waiting here, and the phone kept going.
            </div>
            <div style={{ display: "flex", gap: "22px", alignItems: "center" }}>
              <span
                onClick={props.onContinueWithApple}
                style={{
                  height: 44,
                  padding: "0 20px",
                  display: "inline-flex",
                  alignItems: "center",
                  background: "var(--ink)",
                  color: "var(--surface)",
                  fontSize: "var(--size-utility-2)",
                  cursor: "pointer",
                }}
              >
                sign in again
              </span>
              <Verb onClick={props.onStopSyncing}>or stop syncing on this mac</Verb>
            </div>
          </>
        ) : null}

        {/*
          Two wordings. Not an error and not a merge: the same moment was worded in two
          places while the devices were apart, both are here, and only the person can say
          which one the record should read.
        */}
        {props.conflicts.length > 0 && props.state === "on" ? (
          <div
            style={{
              marginTop: "10px",
              display: "flex",
              flexDirection: "column",
              gap: "14px",
              borderTop: "1px solid var(--rule-dim)",
              paddingTop: "20px",
            }}
          >
            <div
              style={{
                ...metaStyle("var(--size-meta-sm)"),
                letterSpacing: "var(--track-tier-label)",
                textTransform: "uppercase",
              }}
            >
              {props.conflicts.length === 1
                ? "one moment was worded twice"
                : `${props.conflicts.length} moments were worded twice`}
            </div>
            <Secondary>
              you corrected the same moment on both devices while they were apart. both
              wordings are here; neither was thrown away. the record shows the one from this
              mac until you say otherwise.
            </Secondary>

            {props.conflicts.map((c) => (
              <div
                key={c.fragmentId}
                style={{ display: "flex", flexDirection: "column", gap: "14px", marginTop: "4px" }}
              >
                {(
                  [
                    ["local", "on this mac", c.localText],
                    ["remote", "on your phone", c.remoteText],
                  ] as const
                ).map(([which, where, text]) => {
                  const shown = c.shows === which;
                  return (
                    <div
                      key={which}
                      style={{
                        display: "flex",
                        flexDirection: "column",
                        gap: "6px",
                        paddingLeft: "18px",
                        borderLeft: shown ? "2px solid var(--ink)" : "2px solid var(--rule)",
                      }}
                    >
                      <div style={metaStyle()}>
                        corrected {where} · {dayLabel(new Date(c.noticedAt), props.now)}{" "}
                        {clockLabel(new Date(c.noticedAt))}
                      </div>
                      <div
                        onClick={() => props.onOpenFragment(c.fragmentId)}
                        style={{
                          fontSize: "var(--size-d1)",
                          lineHeight: 1.3,
                          color: "var(--ink)",
                          fontVariationSettings: "'wdth' 94",
                          cursor: "pointer",
                        }}
                      >
                        {text}
                      </div>
                      <div style={metaStyle("var(--size-meta-lg)")}>
                        {shown ? (
                          <span>shown in the record now</span>
                        ) : (
                          <Verb
                            ink
                            onClick={() => props.onResolveConflict(c.fragmentId, which)}
                          >
                            show this one instead
                          </Verb>
                        )}
                      </div>
                    </div>
                  );
                })}
                <div style={metaStyle("var(--size-meta-lg)")}>
                  <Verb ink onClick={() => props.onResolveConflict(c.fragmentId, c.shows)}>
                    that’s settled
                  </Verb>{" "}
                  · either way the other stays under the moment as earlier wording, where you
                  can always read it.
                </div>
              </div>
            ))}
          </div>
        ) : null}

        <div style={{ marginTop: "18px", ...metaStyle(), color: "var(--faint)" }}>
          the whole record is on every device. a correction in two places at once is not a
          problem to solve — both wordings are kept and you choose which one shows. removing
          a moment on one device removes it everywhere, with the same eight seconds to bring
          it back.
        </div>
      </div>
    </div>
  );
}

/** "now", "a moment ago", "offline · since 16:40", "last seen 16:38". */
function lastSeen(d: SyncDevice, offline: boolean, now: Date): string {
  if (!d.lastSeenAt) return d.isThisDevice ? "now" : "not seen yet";
  if (d.isThisDevice) {
    return offline ? `offline · since ${clockLabel(d.lastSeenAt)}` : "now";
  }
  const minutes = (now.getTime() - d.lastSeenAt.getTime()) / 60_000;
  if (offline) return `last seen ${clockLabel(d.lastSeenAt)}`;
  if (minutes < 3) return "a moment ago";
  if (minutes < 60) return `${Math.round(minutes)} minutes ago`;
  return `${dayLabel(d.lastSeenAt, now)} ${clockLabel(d.lastSeenAt)}`;
}

function Head({ children }: { children: ReactNode }) {
  return (
    <div
      style={{
        fontSize: "var(--size-utility-head)",
        lineHeight: 1.24,
        letterSpacing: "var(--track-d0)",
        color: "var(--ink)",
      }}
    >
      {children}
    </div>
  );
}

function Secondary({ children }: { children: ReactNode }) {
  return (
    <div
      style={{
        fontSize: "var(--size-utility-2)",
        lineHeight: 1.45,
        color: "var(--meta)",
        fontVariationSettings: "'wdth' 92",
      }}
    >
      {children}
    </div>
  );
}

/** A confirmation that happens where the verb is, rather than in a dialog over everything. */
function InPlace({ children, indent = true }: { children: ReactNode; indent?: boolean }) {
  return (
    <div
      style={{
        fontSize: "var(--size-utility-2)",
        lineHeight: 1.45,
        color: "var(--ink-far)",
        paddingLeft: indent ? "26px" : 0,
        marginTop: "10px",
        fontVariationSettings: "'wdth' 92",
      }}
    >
      {children}
    </div>
  );
}

function Step({ children }: { children: ReactNode }) {
  return <span style={{ color: "var(--meta)", marginRight: "14px" }}>{children}</span>;
}

function Verb({
  children,
  onClick,
  ink = false,
  tone,
}: {
  children: ReactNode;
  onClick: () => void;
  ink?: boolean;
  tone?: string;
}) {
  return (
    <span
      className="chinotto-verb"
      onClick={onClick}
      style={{ color: tone ?? (ink ? "var(--ink)" : "var(--meta)"), cursor: "pointer" }}
    >
      {children}
    </span>
  );
}

/**
 * The pairing code, drawn locally.
 *
 * A real code rather than a placeholder: the phone has to be able to scan it, and a square
 * that only looks like a QR is the one element on this surface that would be purely
 * decorative. Drawn here rather than fetched, so there is nothing to wait for and no empty
 * square while a request decides whether it will answer.
 */
function QrTile({ url }: { url: string }) {
  return (
    <div
      style={{
        width: 168,
        height: 168,
        flex: "none",
        // Fixed ink-on-paper rather than the appearance's tokens: a camera has to read
        // this, and `fill` is a presentation attribute that would not resolve a CSS
        // variable anyway. High contrast is the only thing that matters here.
        background: "#e6e6e3",
        border: "1px solid var(--rule-dim)",
        boxSizing: "border-box",
        padding: "10px",
      }}
    >
      <QRCode
        value={url}
        size={146}
        bgColor="#e6e6e3"
        fgColor="#141416"
        style={{ width: "100%", height: "100%" }}
      />
    </div>
  );
}
