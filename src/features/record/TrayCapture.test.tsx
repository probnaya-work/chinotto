/**
 * Every state the identity file draws under the glyph, and what each one does to the Record.
 *
 * The panel is the one surface in the product that has no window around it and no way back
 * if it gets something wrong: it opens over whatever you were doing, takes one thought, and
 * closes. So these are about the two things that cannot be allowed to drift — which state
 * is drawn, and which canonical call each state makes.
 */

import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Fragment, ReturnValue } from "../../lib/recordApi";

const listeners = new Map<string, (e: { payload: unknown }) => void>();
let focusChanged: ((e: { payload: boolean }) => void) | null = null;

const hide = vi.fn(() => Promise.resolve());

vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn((name: string, fn: (e: { payload: unknown }) => void) => {
    listeners.set(name, fn);
    return Promise.resolve(() => listeners.delete(name));
  }),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    hide,
    isFocused: () => Promise.resolve(false),
    onFocusChanged: (fn: (e: { payload: boolean }) => void) => {
      focusChanged = fn;
      return Promise.resolve(() => {
        focusChanged = null;
      });
    },
  }),
}));

vi.mock("@/lib/uiZoom", () => ({ applyStoredUiZoom: () => Promise.resolve(1) }));
vi.mock("@/lib/firebaseConfig", () => ({ isFirebaseSyncConfigured: () => true }));
vi.mock("@/lib/appearance", () => ({
  applyAppearance: vi.fn(),
  readAppearance: () => "system",
  readLiftContrast: () => false,
}));

vi.mock("@/lib/recordApi", () => ({
  captureFragment: vi.fn(),
  continueFragment: vi.fn(),
  linkContinuation: vi.fn(() => Promise.resolve()),
  selectReturn: vi.fn(() => Promise.resolve(null)),
  recordReturnOutcome: vi.fn(() => Promise.resolve()),
  refreshTray: vi.fn(() => Promise.resolve()),
  fitCapturePopover: vi.fn(() => Promise.resolve()),
  recordVoice: vi.fn(() => new Promise(() => {})),
  captureVoice: vi.fn(),
  recordTranscript: vi.fn(),
  stopVoiceCapture: vi.fn(() => Promise.resolve()),
}));

// The hook the panel shares with the edge reaches for the api through its own relative
// specifier, so both spellings have to resolve to the one mock.
vi.mock("../../lib/recordApi", async () => await import("@/lib/recordApi"));

import * as api from "@/lib/recordApi";
import { TrayCapture } from "./TrayCapture";

const FRAGMENT: Fragment = {
  id: "older-1",
  body: "a tool should ask for the room it needs",
  capturedAt: "2026-03-14T09:00:00Z",
  captureMethod: "typed",
  captureOrigin: "desktop",
  correctedAt: null,
  correctionCount: 0,
  legacyEditCount: 0,
};

const RETURN: ReturnValue = {
  id: 7,
  fragment: FRAGMENT,
  reason: "repeated_language",
  evidence: [
    { kind: "shared_phrase", detail: "the room it needs", occurredAt: null, relatedId: null },
  ],
  because: { ...FRAGMENT, id: "cause-1", body: "the tray keeps growing" },
};

function field(): HTMLTextAreaElement {
  return screen.getByLabelText(/leave a fragment|continue this/i) as HTMLTextAreaElement;
}

/** The panel is shown by the tray, not by mounting: the webview is never torn down. */
async function open() {
  await act(async () => {
    listeners.get("chinotto-tray-opened")?.({ payload: null });
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  listeners.clear();
  focusChanged = null;
  vi.mocked(api.selectReturn).mockResolvedValue(null);
  vi.mocked(api.recordVoice).mockReturnValue(new Promise(() => {}) as never);
  window.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
});

afterEach(() => vi.restoreAllMocks());

describe("resting", () => {
  it("draws the caret, the placeholder, and the way to speak", async () => {
    render(<TrayCapture />);
    await act(async () => {});

    expect(field()).toHaveAttribute("placeholder", "anything");
    expect(screen.getByText("⏎ leave it")).toBeInTheDocument();
    expect(screen.getByText("esc close")).toBeInTheDocument();
    expect(screen.getByText("hold ⌥ to speak")).toBeInTheDocument();
  });

  it("holds the field's own caret back while the drawn one is standing there", async () => {
    render(<TrayCapture />);
    await act(async () => {});

    expect(field().style.caretColor).toBe("transparent");
    fireEvent.change(field(), { target: { value: "a" } });
    expect(field().style.caretColor).toBe("var(--ink)");
  });

  it("does not ask the Record for a Return merely because it mounted", async () => {
    render(<TrayCapture />);
    await act(async () => {});
    expect(api.selectReturn).not.toHaveBeenCalled();

    await open();
    expect(api.selectReturn).toHaveBeenCalledTimes(1);
  });
});

describe("typing", () => {
  it("leaves a fragment typed from the menu bar, and clears before anything else", async () => {
    vi.mocked(api.captureFragment).mockResolvedValue({ ...FRAGMENT, id: "new-1" });
    render(<TrayCapture />);
    await act(async () => {});

    fireEvent.change(field(), { target: { value: "  the tray keeps growing  " } });
    await act(async () => {
      fireEvent.keyDown(field(), { key: "Enter" });
    });

    expect(api.captureFragment).toHaveBeenCalledWith(
      "the tray keeps growing",
      "typed",
      "menubar",
    );
    expect(field().value).toBe("");
    await waitFor(() => expect(screen.getByText("left.")).toBeInTheDocument());
  });

  it("keeps the words and says so when the save fails", async () => {
    vi.mocked(api.captureFragment).mockRejectedValue(new Error("no database"));
    render(<TrayCapture />);
    await act(async () => {});

    fireEvent.change(field(), { target: { value: "worth keeping" } });
    await act(async () => {
      fireEvent.keyDown(field(), { key: "Enter" });
    });

    await waitFor(() => expect(field().value).toBe("worth keeping"));
    expect(screen.getByText("no database")).toBeInTheDocument();
    expect(hide).not.toHaveBeenCalled();
  });

  it("closes on escape without leaving anything behind", async () => {
    render(<TrayCapture />);
    await act(async () => {});
    fireEvent.keyDown(field(), { key: "Escape" });
    expect(hide).toHaveBeenCalled();
    expect(api.captureFragment).not.toHaveBeenCalled();
  });
});

describe("a return is waiting", () => {
  beforeEach(() => vi.mocked(api.selectReturn).mockResolvedValue(RETURN));

  it("sits above the caret, never instead of it", async () => {
    render(<TrayCapture />);
    await open();

    // `Marked` gives the shared run its own node, so the body arrives in pieces — which is
    // itself the point: the words the two share are highlighted here as they are in the
    // window.
    expect(screen.getByText(/a tool should ask for/)).toBeInTheDocument();
    expect(screen.getByText("the room it needs")).toBeInTheDocument();
    expect(screen.getByText(/back from/)).toBeInTheDocument();
    expect(screen.getByText(/because/)).toBeInTheDocument();
    // The field is still there, and still the thing you are typing into.
    expect(field()).toBeInTheDocument();
    expect(field()).toHaveAttribute("placeholder", "anything");
  });

  it("continues onto the same line, against the same Return the window would answer", async () => {
    vi.mocked(api.continueFragment).mockResolvedValue({ ...FRAGMENT, id: "cont-1" });
    render(<TrayCapture />);
    await open();

    await act(async () => {
      screen.getByRole("button", { name: "continue" }).click();
    });
    expect(field()).toHaveAttribute("placeholder", "continue it");
    expect(screen.getByText("⏎ continue it")).toBeInTheDocument();

    fireEvent.change(field(), { target: { value: "and no more" } });
    await act(async () => {
      fireEvent.keyDown(field(), { key: "Enter" });
    });

    expect(api.continueFragment).toHaveBeenCalledWith(
      "older-1",
      "and no more",
      "typed",
      "menubar",
    );
    expect(api.recordReturnOutcome).toHaveBeenCalledWith(7, "continued");
    expect(api.captureFragment).not.toHaveBeenCalled();
  });

  it("puts it back as an outcome rather than a deletion", async () => {
    render(<TrayCapture />);
    await open();

    await act(async () => {
      screen.getByRole("button", { name: "put it back" }).click();
    });
    expect(api.recordReturnOutcome).toHaveBeenCalledWith(7, "let_go");
    expect(screen.queryByText(/a tool should ask for/)).not.toBeInTheDocument();
  });

  it("steps back out of continuing before it closes, so escape never answers a Return", async () => {
    render(<TrayCapture />);
    await open();
    await act(async () => {
      screen.getByRole("button", { name: "continue" }).click();
    });

    fireEvent.keyDown(field(), { key: "Escape" });
    expect(hide).not.toHaveBeenCalled();
    expect(api.recordReturnOutcome).not.toHaveBeenCalled();
    expect(field()).toHaveAttribute("placeholder", "anything");

    fireEvent.keyDown(field(), { key: "Escape" });
    expect(hide).toHaveBeenCalled();
  });
});

describe("speaking", () => {
  it("holds ⌥ on an empty field, and says what it is doing", async () => {
    render(<TrayCapture />);
    await act(async () => {});

    await act(async () => {
      fireEvent.keyDown(field(), { key: "Alt" });
    });

    expect(api.recordVoice).toHaveBeenCalledTimes(1);
    expect(screen.getByText(/holding ⌥ ·/)).toBeInTheDocument();
    expect(screen.getByText("release to leave it")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "esc drop it" })).toBeInTheDocument();
  });

  it("does not start over words that are already there", async () => {
    render(<TrayCapture />);
    await act(async () => {});

    fireEvent.change(field(), { target: { value: "already typing" } });
    await act(async () => {
      fireEvent.keyDown(field(), { key: "Alt" });
    });
    expect(api.recordVoice).not.toHaveBeenCalled();
  });

  it("keeps nothing when the hold is dropped with escape", async () => {
    render(<TrayCapture />);
    await act(async () => {});
    await act(async () => {
      fireEvent.keyDown(field(), { key: "Alt" });
    });

    await act(async () => {
      fireEvent.keyDown(window, { key: "Escape" });
    });
    expect(api.stopVoiceCapture).toHaveBeenCalledTimes(1);
    expect(api.captureVoice).not.toHaveBeenCalled();
  });

  it("takes the chord from anywhere on the mac while it is the surface in front", async () => {
    render(<TrayCapture />);
    await act(async () => {});

    await act(async () => {
      listeners.get("chinotto-voice-hold-start")?.({ payload: null });
    });
    expect(api.recordVoice).toHaveBeenCalledTimes(1);

    await act(async () => {
      listeners.get("chinotto-voice-hold-stop")?.({ payload: null });
    });
    expect(api.stopVoiceCapture).toHaveBeenCalledTimes(1);
  });
});

describe("the menu bar it belongs to", () => {
  it("is told after every capture, so `today · n` is right without a window", async () => {
    vi.mocked(api.captureFragment).mockResolvedValue({ ...FRAGMENT, id: "new-1" });
    render(<TrayCapture />);
    await act(async () => {});
    vi.mocked(api.refreshTray).mockClear();

    fireEvent.change(field(), { target: { value: "one more" } });
    await act(async () => {
      fireEvent.keyDown(field(), { key: "Enter" });
    });
    expect(api.refreshTray).toHaveBeenCalledWith(true);
  });
});

describe("dismissal", () => {
  it("closes once the window really has lost focus, not on the first blur", async () => {
    vi.useFakeTimers();
    render(<TrayCapture />);
    await act(async () => {});

    act(() => focusChanged?.({ payload: false }));
    expect(hide).not.toHaveBeenCalled();

    await act(async () => {
      vi.advanceTimersByTime(300);
    });
    await vi.waitFor(() => expect(hide).toHaveBeenCalled());
    vi.useRealTimers();
  });
});
