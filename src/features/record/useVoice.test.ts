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
  // Never resolves: the recording is still running for the whole of each test, which is
  // exactly the state a release has to be able to get out of.
  recordVoice: vi.fn(() => new Promise(() => {})),
  captureVoice: vi.fn(),
  recordTranscript: vi.fn(),
  stopVoiceCapture: vi.fn(() => Promise.resolve()),
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
