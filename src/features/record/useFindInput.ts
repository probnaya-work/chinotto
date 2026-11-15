import { useCallback, useRef, useState } from 'react';

/** Find borrows the capture field; leaving it restores the exact unfinished capture. */
export function useFindInput() {
  const [input, setValue] = useState('');
  // Native submit can arrive before React renders the preceding keystroke.
  const inputNow = useRef('');
  const captureDraft = useRef<string | null>(null);
  const setInput = useCallback((text: string) => {
    if (inputNow.current.startsWith('/') && !text.startsWith('/') && captureDraft.current !== null) {
      text = captureDraft.current;
      captureDraft.current = null;
    }
    inputNow.current = text;
    setValue(text);
  }, []);
  const startFind = useCallback(() => {
    if (inputNow.current.startsWith('/')) return;
    captureDraft.current = inputNow.current;
    setInput('/');
  }, [setInput]);
  const leaveFind = useCallback(() => {
    const draft = captureDraft.current ?? '';
    captureDraft.current = null;
    setInput(draft);
  }, [setInput]);
  return { input, inputNow, setInput, startFind, leaveFind, finding: input.startsWith('/') };
}
