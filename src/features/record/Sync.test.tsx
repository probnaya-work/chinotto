import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Sync, type SyncProps } from "./Sync";

function props(overrides: Partial<SyncProps> = {}): SyncProps {
  return {
    state: "off",
    offline: false,
    pending: 0,
    devices: null,
    conflicts: [],
    now: new Date("2026-10-04T20:00:00Z"),
    phoneReady: false,
    onAlreadyDone: vi.fn(),
    onContinueWithApple: vi.fn(),
    onCancelSignIn: vi.fn(),
    qrUrl: "https://getchinotto.app/sync",
    onRemoveDevice: vi.fn(),
    onStopSyncing: vi.fn(),
    onResolveConflict: vi.fn(),
    onOpenFragment: vi.fn(),
    errorMessage: null,
    onLeave: vi.fn(),
    ...overrides,
  };
}

describe("Sync", () => {
  it("lets Continue with Apple work on the first click", () => {
    const onContinueWithApple = vi.fn();
    render(<Sync {...props({ onContinueWithApple })} />);

    const button = screen.getByRole("button", { name: "continue with apple" });
    expect(button).toBeEnabled();
    expect(button).toHaveClass("chinotto-verb--filled");
    fireEvent.click(button);
    expect(onContinueWithApple).toHaveBeenCalledOnce();
  });

  it("keeps the recovery action visibly filled after a sign-in error", () => {
    render(<Sync {...props({ state: "error", errorMessage: "The network went away." })} />);

    expect(screen.getByRole("button", { name: "try apple sign-in again" })).toHaveClass(
      "chinotto-verb--filled",
    );
  });

  it("offers an immediate escape from a sign-in request that lost its system sheet", () => {
    const onCancelSignIn = vi.fn();
    render(<Sync {...props({ state: "connecting", onCancelSignIn })} />);

    fireEvent.click(screen.getByRole("button", { name: "cancel and try again" }));
    expect(onCancelSignIn).toHaveBeenCalledOnce();
  });

  it("shows the actual sign-in failure instead of inventing an expired session", () => {
    render(<Sync {...props({ state: "error", errorMessage: "The network went away." })} />);

    expect(screen.getByText(/The network went away/)).toBeInTheDocument();
    expect(screen.queryByText(/sign-in expired/i)).not.toBeInTheDocument();
  });
});
