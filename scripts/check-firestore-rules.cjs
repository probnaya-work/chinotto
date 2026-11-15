#!/usr/bin/env node
// Exercise synthetic requests in Firebase's rules evaluator; no documents are read or written.
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');

async function main() {
  const root = path.resolve(__dirname, '..');
  const project = JSON.parse(fs.readFileSync(path.join(root, '.firebaserc'), 'utf8')).projects.default;
  if (project !== 'chinotto') throw new Error('Expected the chinotto Firebase project');
  const sourcePath = process.argv[2] || path.join(root, 'firestore.rules');
  const content = fs.readFileSync(sourcePath, 'utf8');
  const firebase = fs.realpathSync(execFileSync('which', ['firebase'], { encoding: 'utf8' }).trim());
  const auth = require(path.resolve(path.dirname(firebase), '..', 'auth.js'));
  const account = auth.getProjectDefaultAccount(root);
  if (!account?.tokens?.refresh_token) throw new Error('Sign in with the existing Firebase CLI account first');
  const credentials = await auth.getAccessToken(account.tokens.refresh_token, ['https://www.googleapis.com/auth/cloud-platform']);

  const owner = 'synthetic-release-owner';
  const contexts = [
    { label: 'owner', auth: { uid: owner, token: {} } },
    { label: 'other account', auth: { uid: 'synthetic-other-account', token: {} } },
    { label: 'signed out', auth: null },
  ];
  const tests = [];
  const add = (label, document, method, context, allow) => tests.push({
    label: `${label} / ${method} / ${context.label}`,
    expectation: allow ? 'ALLOW' : 'DENY',
    request: { path: `/databases/(default)/documents/${document}`, method, auth: context.auth },
  });
  for (const [label, document, known] of [
    ['profile', `users/${owner}`, true],
    ['entry', `users/${owner}/entries/synthetic-entry`, true],
    ['device', `users/${owner}/devices/synthetic-device`, true],
    ['legacy theme', `users/${owner}/user_themes/synthetic-theme`, true],
    ['unknown subcollection', `users/${owner}/unknown/synthetic-document`, false],
  ]) {
    for (const method of ['get', 'list', 'create', 'update', 'delete']) {
      for (const context of contexts) add(label, document, method, context, known && context.label === 'owner');
    }
  }
  // Keep the existing sign-in-session contract and deny unrelated collection access.
  for (const context of contexts) {
    for (const method of ['get', 'list', 'create', 'update', 'delete']) {
      add('desktop sign-in session', 'sync_desktop_sessions/synthetic-session', method, context,
        method === 'get' || method === 'list' || ((method === 'create' || method === 'update') && context.auth !== null));
      add('unrelated collection', 'unknown/synthetic-document', method, context, false);
    }
  }

  const response = await fetch(`https://firebaserules.googleapis.com/v1/projects/${project}:test`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${credentials.access_token}`, 'Content-Type': 'application/json' },
    body: JSON.stringify({
      source: { files: [{ name: 'firestore.rules', content }] },
      testSuite: { testCases: tests.map(({ label, ...test }) => test) },
    }),
  });
  const result = await response.json();
  if (!response.ok) throw new Error(`Firebase rules evaluator returned ${response.status}: ${result.error?.message || 'unknown error'}`);
  const issues = (result.issues || []).filter((issue) => issue.severity === 'ERROR');
  const failures = tests.filter((_, index) => result.testResults?.[index]?.state !== 'SUCCESS');
  for (const issue of issues) console.error(issue.description || JSON.stringify(issue));
  for (const failure of failures) console.error(`Failed: ${failure.label} (expected ${failure.expectation})`);
  if (issues.length || failures.length) throw new Error(`${failures.length}/${tests.length} security checks failed`);
  console.log(`${tests.length} Firebase rules checks passed; owner access, account isolation and signed-out denial verified`);
}

main().catch((error) => { console.error(error.message); process.exitCode = 1; });
