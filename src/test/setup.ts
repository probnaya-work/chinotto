import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";

// Node's built-in Web Storage can shadow the browser storage in Vitest 2.
const { window: browser } = (globalThis as typeof globalThis & {
  jsdom: { window: Window };
}).jsdom;
Object.defineProperties(globalThis, {
  localStorage: { configurable: true, value: browser.localStorage },
  sessionStorage: { configurable: true, value: browser.sessionStorage },
});

afterEach(() => cleanup());
