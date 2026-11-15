import { getCurrentWindow } from '@tauri-apps/api/window';
import type { Appearance } from './appearance';

let changes: Promise<void> = Promise.resolve();

/** The main window owns macOS's app-wide appearance; transparent capture stays transparent. */
export function syncNativeAppearance(appearance: Appearance): Promise<void> {
  if (!('__TAURI_INTERNALS__' in window)) return Promise.resolve();
  changes = changes.catch(() => {}).then(async () => {
    const win = getCurrentWindow();
    if (win.label !== 'main') return;
    await win.setTheme(appearance === 'system' ? null : appearance);
    const surface = getComputedStyle(document.documentElement).getPropertyValue('--surface').trim();
    if (surface) await win.setBackgroundColor(surface);
  });
  return changes;
}
