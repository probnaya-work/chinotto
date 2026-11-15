import { useCallback, useRef, useState } from 'react';

/** Find borrows the capture field; leaving it restores the exact unfinished capture. */
export function useFindInput() {
  const [input, setValue] = useState('');
  // Native submit can arrive before React renders the preceding keystroke.
  const inputNow = useRef('');
  const captureDraft = useRef<string | null>(null);
  // A slash typed in the protected capture field is wording until that field is left.
  const literalCapture = useRef(false);
  const [borrowedCapture, setBorrowedCapture] = useState('');
  const setInput = useCallback((text: string) => {
    if (inputNow.current.startsWith('/') && !text.startsWith('/') && captureDraft.current !== null) {
      text = captureDraft.current;
      captureDraft.current = null;
    }
    inputNow.current = text;
    setValue(text);
  }, []);
  const startFind = useCallback(() => {
    literalCapture.current = false;
    if (inputNow.current.startsWith('/')) return;
    captureDraft.current = inputNow.current;
    setBorrowedCapture(inputNow.current);
    setInput('/');
  }, [setInput]);
  const leaveFind = useCallback(() => {
    literalCapture.current = false;
    const draft = captureDraft.current ?? '';
    captureDraft.current = null;
    setInput(draft);
  }, [setInput]);
  const setCaptureInput = useCallback((text: string) => {
    if (literalCapture.current || !inputNow.current.startsWith('/')) {
      literalCapture.current = text.startsWith('/');
      setInput(text);
    } else if (inputNow.current.startsWith('/')) {
      captureDraft.current = text;
      setBorrowedCapture(text);
    }
  }, [setInput]);
  return { input, inputNow, setInput, startFind, leaveFind, finding: input.startsWith('/'),
    captureInput: input.startsWith('/') && !literalCapture.current ? (captureDraft.current !== null ? borrowedCapture : '') : input, setCaptureInput };

}
