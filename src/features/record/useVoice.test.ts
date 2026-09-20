/**
 * A hold has to be releasable.
 *
 * The press arrives on whatever the pointer or the caret was on; the release very often
 * does not. Slide the pointer off `◌ hold space to speak` before letting go, or let the
 * field lose focus mid-hold, and a release listened for on that element never comes — and
 * the recording then runs to its two-minute ceiling with no way to end it. These pin the
 * release to the window, which is the only thing that is certainly still there.
 */

import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useVoice } from "./useVoice";
import * as api from "../../lib/recordApi";

vi.mock("../../lib/recordApi", () => ({
  // Never resolves by default: the recording is still running for the whole of each test,
  // which is exactly the state a release has to be able to get out of.
  recordVoice: vi.fn(() => new Promise(() => {})),
  captureVoice: vi.fn(),
  recordTranscript: vi.fn(),
  stopVoiceCapture: vi.fn(() => Promise.resolve()),
  linkContinuation: vi.fn(() => Promise.resolve()),
}));

const stopped = () => vi.mocked(api.stopVoiceCapture).mock.calls.length;

describe("ending a recording", () => {
  beforeEach(() => vi.mocked(api.stopVoiceCapture).mockClear());
  afterEach(() => vi.restoreAllMocks());

  function holding() {
    const hook = renderHook(() => useVoice(() => {}));
    act(() => hook.result.current.start());
    expect(hook.result.current.recording).toBe(true);
    return hook;
  }

  it("ends when space comes up anywhere, not only where it went down", () => {
    const hook = holding();
    act(() => {
      window.dispatchEvent(new KeyboardEvent("keyup", { key: " " }));
    });
    expect(stopped()).toBe(1);
    hook.unmount();
  });

  it("ends when the mouse comes up anywhere, not only on the hint", () => {
    const hook = holding();
    act(() => {
      window.dispatchEvent(new MouseEvent("mouseup"));
    });
    expect(stopped()).toBe(1);
    hook.unmount();
  });

  it("ends when the window stops being the one that would hear the release", () => {
    const hook = holding();
    act(() => {
      window.dispatchEvent(new Event("blur"));
    });
    expect(stopped()).toBe(1);
    hook.unmount();
  });

  it("does not let the held key page the record away underneath it", () => {
    const hook = holding();

    // The field that swallowed the first press is gone — the waveform replaced it — so the
    // auto-repeats of the hold arrive at the document, where `space` means "page down".
    const repeat = new KeyboardEvent("keydown", { key: " ", cancelable: true });
    act(() => {
      window.dispatchEvent(repeat);
    });
    expect(repeat.defaultPrevented).toBe(true);
    expect(stopped()).toBe(0);
    hook.unmount();
  });

  it("leaves other keys' defaults alone", () => {
    const hook = holding();
    const other = new KeyboardEvent("keydown", { key: "PageDown", cancelable: true });
    act(() => {
      window.dispatchEvent(other);
    });
    expect(other.defaultPrevented).toBe(false);
    hook.unmount();
  });

  it("stops holding the key's default once the recording is over", () => {
    const hook = renderHook(() => useVoice(() => {}));
    const idle = new KeyboardEvent("keydown", { key: " ", cancelable: true });
    act(() => {
      window.dispatchEvent(idle);
    });
    expect(idle.defaultPrevented).toBe(false);
    hook.unmount();
  });

  it("is not ended by some other key coming up", () => {
    const hook = holding();
    act(() => {
      window.dispatchEvent(new KeyboardEvent("keyup", { key: "a" }));
    });
    expect(stopped()).toBe(0);
    hook.unmount();
  });

  it("listens for nothing while no recording is running", () => {
    const hook = renderHook(() => useVoice(() => {}));
    act(() => {
      window.dispatchEvent(new KeyboardEvent("keyup", { key: " " }));
      window.dispatchEvent(new MouseEvent("mouseup"));
    });
    expect(stopped()).toBe(0);
    hook.unmount();
  });
});

/**
 * The menu-bar panel holds the same recorder the edge does, and the difference between them
 * is three things: which key ends the hold, which origin the fragment carries, and what may
 * happen to the fragment before the transcript is attached.
 */
describe("the same recorder, held from the menu bar", () => {
  beforeEach(() => {
    vi.mocked(api.stopVoiceCapture).mockClear();
    vi.mocked(api.captureVoice).mockReset();
    vi.mocked(api.recordTranscript).mockReset();
  });
  afterEach(() => vi.restoreAllMocks());

  /** A hold that really ends, so the capture runs to a fragment. */
  function recorded(durationMs: number) {
    vi.mocked(api.recordVoice).mockReturnValueOnce(
      Promise.resolve({
        audioPath: "/tmp/held.m4a",
        durationMs,
        transcript: "the tray keeps growing",
        transcriptFailure: null,
      }) as ReturnType<typeof api.recordVoice>,
    );
    // Each test queues its own `captureVoice`, so that the one test about ordering can
    // watch it rather than inherit an answer queued here first.
    vi.mocked(api.captureVoice).mockResolvedValue({
      id: "spoken-1",
    } as Awaited<ReturnType<typeof api.captureVoice>>);
    vi.mocked(api.recordTranscript).mockResolvedValue(undefined);
  }

  it("ends on the key the surface says it ends on, and not on the edge's", () => {
    const hook = renderHook(() =>
      useVoice(() => {}, { origin: "menubar", releaseKey: "Alt" }),
    );
    act(() => hook.result.current.start());

    act(() => window.dispatchEvent(new KeyboardEvent("keyup", { key: " " })));
    expect(stopped()).toBe(0);

    act(() => window.dispatchEvent(new KeyboardEvent("keyup", { key: "Alt" })));
    expect(stopped()).toBe(1);
    hook.unmount();
  });

  it("carries the menu bar into the fragment it makes", async () => {
    recorded(4200);
    const hook = renderHook(() =>
      useVoice(() => {}, { origin: "menubar", releaseKey: "Alt" }),
    );
    await act(async () => {
      hook.result.current.start();
    });
    expect(vi.mocked(api.captureVoice)).toHaveBeenCalledWith("/tmp/held.m4a", 4200, "menubar");
    hook.unmount();
  });

  it("makes the recording a fragment before it reaches for the words", async () => {
    recorded(4200);
    const order: string[] = [];
    vi.mocked(api.captureVoice).mockImplementation(async () => {
      order.push("fragment");
      return { id: "spoken-1" } as Awaited<ReturnType<typeof api.captureVoice>>;
    });
    vi.mocked(api.recordTranscript).mockImplementation(async () => {
      order.push("transcript");
    });
    const hook = renderHook(() => useVoice(() => {}, { origin: "menubar" }));
    await act(async () => {
      hook.result.current.start();
    });
    expect(order).toEqual(["fragment", "transcript"]);
    hook.unmount();
  });

  it("keeps the recording when the transcript cannot be stored", async () => {
    recorded(4200);
    vi.mocked(api.recordTranscript).mockRejectedValue(new Error("no recogniser"));
    const captured = vi.fn();
    const hook = renderHook(() => useVoice(captured, { origin: "menubar" }));
    await act(async () => {
      hook.result.current.start();
    });
    expect(vi.mocked(api.captureVoice)).toHaveBeenCalledTimes(1);
    expect(captured).toHaveBeenCalledTimes(1);
    hook.unmount();
  });

  it("gives the fragment to the surface before the transcript, so a line can be joined", async () => {
    recorded(4200);
    const seen: string[] = [];
    const hook = renderHook(() =>
      useVoice(() => {}, {
        origin: "menubar",
        onFragment: (id) => {
          seen.push(id);
        },
      }),
    );
    await act(async () => {
      hook.result.current.start();
    });
    expect(seen).toEqual(["spoken-1"]);
    hook.unmount();
  });

  it("keeps nothing when the hold is dropped", async () => {
    recorded(4200);
    const captured = vi.fn();
    const hook = renderHook(() => useVoice(captured, { origin: "menubar" }));
    act(() => hook.result.current.start());
    await act(async () => {
      hook.result.current.drop();
    });
    expect(stopped()).toBe(1);
    expect(vi.mocked(api.captureVoice)).not.toHaveBeenCalled();
    expect(captured).not.toHaveBeenCalled();
    hook.unmount();
  });

  it("keeps the recording when the hold is merely released", async () => {
    recorded(4200);
    const hook = renderHook(() => useVoice(() => {}, { origin: "menubar" }));
    await act(async () => {
      hook.result.current.start();
    });
    expect(vi.mocked(api.captureVoice)).toHaveBeenCalledTimes(1);
    hook.unmount();
  });
});
