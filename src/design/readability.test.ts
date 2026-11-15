import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import light from './lightPalette.json';
import { applyTextScale } from '../lib/appearance';
const luminance = (hex: string) => {
  const channels = hex.slice(1).match(/../g)!.map((v) => parseInt(v, 16) / 255).map((v) => v <= .04045 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4);
  return channels[0] * .2126 + channels[1] * .7152 + channels[2] * .0722;
};
const ratio = (ink: string, surface: string) => (luminance(surface) + .05) / (luminance(ink) + .05);
describe('light reading palette', () => {
  it.each(['ink', 'near', 'far', 'dim', 'meta', 'quoted', 'agency', 'agencyQuiet', 'live', 'placeholder'] as const)('%s remains readable on the mineral field', (key) => {
    expect(ratio(light[key], light.surface)).toBeGreaterThanOrEqual(4.5);
  });
  it('keeps distance ordered while bringing every text tier above the contrast floor', () => {
    const tones = ['ink', 'near', 'far', 'dim', 'meta'] as const;
    const levels = tones.map((key) => luminance(light[key]));
    expect(levels).toEqual([...levels].sort((a, b) => a - b));
  });
  it('uses the same reading colours in explicit and system light appearances', () => {
    const css = readFileSync('src/design/tokens.css', 'utf8');
    const blocks = [...css.matchAll(/:root(?:\[data-theme="light"\]|:not\(\[data-theme="dark"\]\))\s*\{([^}]+)/g)];
    expect(blocks).toHaveLength(2);
    for (const [, block] of blocks) {
      expect(block).toContain(`--surface: ${light.surface}`);
      expect(block).toContain(`--ink: ${light.ink}`);
      expect(block).toContain(`--meta: ${light.meta}`);
      expect(block).toContain(`--evidence-ground: ${light.evidenceGround}`);
    }
  });
  it('does not apply the light contrast override to a system-dark window', () => {
    const css = readFileSync('src/design/tokens.css', 'utf8');
    expect(css).not.toContain(':root[data-lift-contrast]:not([data-theme="dark"]),');
    const system = css.lastIndexOf('@media (prefers-color-scheme: light)');
    expect(css.slice(system)).toContain(':root[data-lift-contrast]:not([data-theme="dark"])');
  });
});

it('relaxes old-text compression only when the person asks for large text', () => {
  applyTextScale(130);
  expect(document.documentElement.hasAttribute('data-large-text')).toBe(true);
  expect(document.documentElement.style.getPropertyValue('--text-scale')).toBe('1.3');
  applyTextScale(100);
  expect(document.documentElement.hasAttribute('data-large-text')).toBe(false);
});
