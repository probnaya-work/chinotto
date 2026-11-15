import { it, expect } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { useFindInput } from './useFindInput';

it('restores the exact capture draft after searching and repeated Find presses', () => {
  const { result } = renderHook(() => useFindInput());
  act(() => { result.current.setInput('unfinished\n  мысль '); result.current.startFind(); });
  expect(result.current.input).toBe('/');
  act(() => { result.current.setInput('/a query'); result.current.startFind(); result.current.leaveFind(); });
  expect(result.current.input).toBe('unfinished\n  мысль ');
  expect(result.current.inputNow.current).toBe('unfinished\n  мысль ');
});
it('returns the draft when the slash command is cleared or removed', () => {
  const { result } = renderHook(() => useFindInput());
  act(() => { result.current.setInput('keep this'); result.current.startFind(); result.current.setInput('/query'); });
  act(() => result.current.setInput('query'));
  expect(result.current.input).toBe('keep this');
  act(() => { result.current.startFind(); result.current.setInput(''); });
  expect(result.current.input).toBe('keep this');
});
it('leaves a manually typed search without turning its query into a capture', () => {
  const { result } = renderHook(() => useFindInput());
  act(() => result.current.setInput('/words'));
  act(() => result.current.leaveFind());
  expect(result.current.input).toBe('');
});
