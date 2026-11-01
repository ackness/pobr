// Use the same identity as build-wasm's validated reuse path, including tools
// and configuration. Never use commit SHA: unrelated UI/data changes can reuse it.
import { appendFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { buildInputs, sha256, sourceFingerprint } from './wasm-build-inputs.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const key = sha256(JSON.stringify([sourceFingerprint(root), buildInputs(root)]));
console.log(`key=${key}`);
if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `key=${key}\n`);
