import { afterEach, expect, it, vi } from 'vitest';
import { syncNativeAppearance } from './nativeAppearance';
const { theme, background, win } = vi.hoisted(() => {
  const theme = vi.fn(async (_choice: string | null) => {});
  const background = vi.fn(async (_color: string) => {});
  return { theme, background, win: { label: 'main', setTheme: theme, setBackgroundColor: background } };
});
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => win }));
afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  document.documentElement.style.removeProperty('--surface');
  vi.clearAllMocks(); win.label = 'main';
});
it('applies the chosen native theme and field in order, including returning to the system', async () => {
  Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true });
  document.documentElement.style.setProperty('--surface', '#eeeae1');
  await Promise.all([syncNativeAppearance('light'), syncNativeAppearance('dark'), syncNativeAppearance('system')]);
  expect(theme.mock.calls.map(([choice]) => choice)).toEqual(['light', 'dark', null]);
  expect(background).toHaveBeenLastCalledWith('#eeeae1');
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});
it('leaves a transparent capture host alone', async () => {
  Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true });
  win.label = 'capture-popover';
  await syncNativeAppearance('light');
  expect(theme).not.toHaveBeenCalled();
  expect(background).not.toHaveBeenCalled();
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});
it('does not call native APIs from a browser preview', async () => {
  await syncNativeAppearance('light');
  expect(theme).not.toHaveBeenCalled();
});
