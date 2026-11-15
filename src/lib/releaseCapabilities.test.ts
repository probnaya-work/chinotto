import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const direct = JSON.parse(readFileSync('src-tauri/capabilities/default.json', 'utf8'));
const mas = JSON.parse(readFileSync('src-tauri/tauri.mas-build.json', 'utf8'));

describe.each([
  ['direct', direct.permissions],
  ['Mac App Store', mas.app.security.capabilities[0].permissions],
] as const)('%s release capabilities', (_variant, permissions) => {
  it('permits the archive picker and native appearance used by the Record', () => {
    // Browser mocks cannot exercise Tauri's invoke ACL. The store build replaces
    // default.json, so a working direct build does not prove these flows can run there.
    expect(permissions).toEqual(expect.arrayContaining([
      'dialog:allow-open',
      'dialog:allow-save',
      'core:window:allow-set-theme',
      'core:window:allow-set-background-color',
    ]));
  });
});
