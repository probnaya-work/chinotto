import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { invokeRecord, recordCanRead, recordEpoch, authenticateRecord } from '../../lib/recordPrivacy';
import { Verb } from './Verb';

export interface RestorePreview {
  newMoments: number; audioFiles: number; newArchived: number; duplicates: number; changed: number;
  removed: number; waitingLinks: number; skippedJudgements: number; from: string | null; to: string | null;
  sample: { body: string; at: string }[];
}
export function RestoreArchive({ protectedRecord, onRestored }: { protectedRecord: boolean; onRestored: () => void }) {
  const [preview, setPreview] = useState<RestorePreview | null>(null);
  const [status, setStatus] = useState('');
  const [busy, setBusy] = useState(false);
  const work = useRef<string | null>(null);
  const discard = (token: string) => { void invoke('discard_record_restore', { token }).catch(() => {}); };
  useEffect(() => () => { if (work.current) discard(work.current); work.current = null; }, []);
  const cancel = () => { if (work.current) discard(work.current); work.current = null; setPreview(null); setStatus(''); setBusy(false); };
  const choose = async () => {
    if (work.current) return;
    const token = crypto.randomUUID(); work.current = token; setBusy(true); setStatus('Reading and checking the archive…');
    try {
      if (protectedRecord && !await authenticateRecord()) { cancel(); return; }
      const epoch = recordEpoch();
      const path = await open({ multiple: false, directory: false, filters: [{ name: 'Chinotto archive', extensions: ['zip'] }] });
      if (work.current !== token) return;
      if (!path) { cancel(); return; }
      if (!recordCanRead() || epoch !== recordEpoch()) throw new Error('Open the Record again before choosing an archive.');
      const prepared = await invokeRecord<{ token: string; preview: RestorePreview }>('prepare_record_restore', { path, token });
      if (work.current !== token) { discard(token); return; }
      setPreview(prepared.preview); setStatus('');
    } catch (error) {
      discard(token);
      if (work.current === token) { work.current = null; setStatus(String(error)); }
    } finally { if (work.current === token || work.current === null) setBusy(false); }
  };
  const restore = async () => {
    const token = work.current;
    if (!token || busy) return;
    setBusy(true); setStatus('Restoring…');
    try {
      const done = await invokeRecord<RestorePreview>('restore_record_archive', { token });
      if (work.current === token) {
        setStatus(`Restored ${done.newMoments} thoughts, ${done.audioFiles} recordings and ${done.newArchived} archived items. Your existing Record was kept.`);
        onRestored();
      }
    } catch (error) { if (work.current === token) setStatus(String(error)); }
    finally { discard(token); if (work.current === token) { work.current = null; setPreview(null); setBusy(false); } }
  };
  return <div style={{ display: 'grid', gap: 10 }}>
    <div>restore from a Chinotto archive. Original dates, history and audio stay together.</div>
    {preview ? <div style={{ display: 'grid', gap: 10 }}>
      <div>{preview.newMoments} thoughts · {preview.audioFiles} recordings · {preview.newArchived} archived items</div>
      {preview.from ? <div>{new Date(preview.from).toLocaleDateString()} — {new Date(preview.to!).toLocaleDateString()}</div> : null}
      <div>{preview.duplicates} already here · {preview.changed} with different wording · {preview.removed} previously removed. Existing thoughts keep their current wording.</div>
      {preview.waitingLinks || preview.skippedJudgements ? <div>{preview.waitingLinks} links have a missing predecessor; {preview.skippedJudgements} trace choices refer to missing thoughts.</div> : null}
      {preview.sample.map((row, i) => <div key={i} style={{ color: 'var(--meta)', overflowWrap: 'anywhere' }}>{row.body || 'voice recording'}</div>)}
      <div style={{ color: 'var(--meta)' }}>A recovery copy of the database is saved on this Mac before restoring.</div>
      {!busy ? <Verb aria-label="Restore archive" onClick={() => void restore()}>restore {preview.newMoments} thoughts ›</Verb> : null}
    </div> : null}
    {status ? <div role="status">{status}</div> : null}
    {work.current ? <Verb onClick={cancel}>cancel</Verb> : <Verb onClick={() => void choose()}>choose archive ›</Verb>}
  </div>;
}
