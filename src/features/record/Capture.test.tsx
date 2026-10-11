import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Capture } from "./Capture";

const base = {
  value: "",
  onChange: vi.fn(),
  onLeave: vi.fn(),
  onStand: vi.fn(),
};

describe("desktop voice gestures", () => {
  it("keeps both halves of a quick space tap on the original field", () => {
    const start = vi.fn();
    const release = vi.fn();
    render(
      <Capture
        {...base}
        onStartSpeaking={start}
        onReleaseSpeaking={release}
      />,
    );

    const field = screen.getByLabelText("Leave a fragment");
    fireEvent.keyDown(field, { key: " " });
    fireEvent.keyUp(field, { key: " " });
    expect(start).toHaveBeenCalledTimes(1);
    expect(release).toHaveBeenCalledTimes(1);
  });

  it("keeps both halves of a quick mouse click on the hint", () => {
    const start = vi.fn();
    const release = vi.fn();
    render(
      <Capture
        {...base}
        onStartSpeaking={start}
        onReleaseSpeaking={release}
      />,
    );

    const hint = screen.getByText("◌ hold space · tap locks");
    fireEvent.mouseDown(hint);
    fireEvent.mouseUp(hint);
    expect(start).toHaveBeenCalledTimes(1);
    expect(release).toHaveBeenCalledTimes(1);
  });

  it("makes a hands-free recording explicitly stoppable by click", () => {
    const finish = vi.fn();
    render(
      <Capture
        {...base}
        speaking
        speakingLocked
        onFinishSpeaking={finish}
      />,
    );

    const instruction = screen.getByText(
      "hands-free · click or space to leave it · esc to drop",
    );
    fireEvent.mouseDown(instruction.parentElement!);
    expect(finish).toHaveBeenCalledTimes(1);
  });
});
