import { useState } from 'react';
import { Capture, type CaptureProps } from './Capture';
import { Verb } from './Verb';
import { metaStyle } from '../../design/tiers';
import { authenticateRecord, type Protection } from '../../lib/recordPrivacy';

export function LockedRecord({ protection, words, onWords, capture, voice }: {
  protection: Protection; words: string; onWords: (value: string) => void;
  capture: (body: string) => Promise<unknown>;
  voice: Pick<CaptureProps, 'onStartSpeaking' | 'onReleaseSpeaking' | 'speaking' | 'speakingLocked' | 'speakingSeconds' | 'onFinishSpeaking' | 'transcript'>;
}) {
  const [message, setMessage] = useState('the record is locked. a new thought can still land.');
  const [settings, setSettings] = useState(false);
  return <div>
    <div style={{ ...metaStyle(), marginBottom: 20 }} role="status">{protection.error ?? message}</div>
    <div style={{ display: 'flex', gap: 24, marginBottom: 38 }}>
      <Verb disabled={protection.busy} onClick={() => void authenticateRecord()}>
        {protection.busy ? 'authenticating…' : 'open the record ›'}
      </Verb>
      <Verb onClick={() => setSettings((open) => !open)}>device protection</Verb>
    </div>
    {settings ? <div style={{ ...metaStyle(), marginBottom: 32 }}>
      <p>Touch ID or your Mac password protects reading. Capture stays open. This does not encrypt the database or exported copies.</p>
      <Verb disabled={protection.busy} onClick={() => void authenticateRecord(protection.enabled ? 'disable' : 'enable')}>
        {protection.enabled ? 'turn protection off ›' : 'turn protection on ›'}
      </Verb>
    </div> : null}
    <Capture value={words} onChange={onWords} captureOnly onStand={() => {}} {...voice}
      onLeave={(body) => { void capture(body).then(() => {
        setMessage('saved on this Mac.');
      }).catch(() => { onWords(body); setMessage('could not save. your words are still here.'); }); }} />
  </div>;
}
