import { describe, expect, it } from 'vitest';
import { sharedCount, sharedRun, tokenize } from './words';

describe('multilingual word evidence', () => {
  it('preserves Cyrillic, accented words, numbers and mixed-language input', () => {
    expect(tokenize('Мысль sobre café №42').map((t) => t.w)).toEqual([
      'мысль', 'sobre', 'café', '42',
    ]);
    expect(tokenize('cafe\u0301')).toEqual(tokenize('café'));
    expect(tokenize('ещё')).toEqual([{ w: 'ещё', s: 'ещё' }]);
    expect(tokenize('🙂 —')).toEqual([]);
  });

  it('recognises an actual Russian phrase', () => {
    expect(sharedRun(
      tokenize('Хочу сохранить спокойное внимание надолго'),
      tokenize('Пытаюсь сохранить спокойное внимание сегодня'),
    )).toMatchObject({ length: 3, a: 'сохранить спокойное внимание' });
  });

  it('does not use common Russian and Portuguese function words as evidence', () => {
    for (const [a, b] of [
      ['это было вчера', 'это было давно'],
      ['para com uma árvore', 'uma com para janela'],
    ]) {
      expect(sharedRun(tokenize(a), tokenize(b))).toBeNull();
      expect(sharedCount(tokenize(a), tokenize(b))).toBe(0);
    }
  });
});
