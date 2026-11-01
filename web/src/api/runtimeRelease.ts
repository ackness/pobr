/** Transport-only adapter: verified JSON strings become existing GameData paths. */
import type { DigestFn } from './dataIntegrity';

export const RELEASE_SCHEMA = 'pobr-runtime-release/v1';
const BLOCK_SCHEMA = 'pobr-runtime-block/v1';
const kinds = ['core', 'rules', 'locale'] as const;
const encoder = new TextEncoder();
const decoder = new TextDecoder('utf-8', { fatal: true });

export interface RuntimeBlockSpec {
  path: string;
  bytes: number;
  sha256: string;
  files: string[];
}

export interface RuntimeRelease {
  schema: typeof RELEASE_SCHEMA;
  version: string;
  target: { game: 'poe2'; game_patch: string; realm: 'international' | 'china' };
  locale: 'zh-CN' | 'zh-TW';
  runtime_identity: string;
  release_id: string;
  files: string[];
  digests: Record<string, string>;
  blocks: RuntimeBlockSpec[];
}

const requireValue: (value: unknown, message: string) => asserts value = (value, message) => {
  if (!value) throw new Error(message);
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const hex = (value: unknown): value is string => typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
const strings = (value: unknown): value is string[] => Array.isArray(value) && value.every(v => typeof v === 'string');
const safePath = (name: string) => !/[\\:]/.test(name) && name.split('/').every(part => part && part !== '.' && part !== '..');
const filePath = (name: string) => safePath(name) && (name === 'manifest.json'
  || /^(base|overlay|generated|overlay-common|i18n)\/.+\.json$/.test(name));
const kindOf = (name: string) => name === 'manifest.json' || name.startsWith('i18n/')
  ? 'locale' : name.startsWith('base/') ? 'core' : 'rules';

/** Release metadata uses only ASCII keys/paths and integers, never source game numbers. */
export function canonical(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (object(value)) return `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}`;
  return JSON.stringify(value);
}

export async function validateRelease(value: unknown, digest: DigestFn): Promise<RuntimeRelease> {
  requireValue(object(value) && value.schema === RELEASE_SCHEMA, 'Unknown runtime release schema');
  const { version, target, locale, files, digests, blocks, runtime_identity, release_id } = value;
  requireValue(typeof version === 'string' && /^[0-9]+(?:\.[0-9]+)+$/.test(version)
    && object(target) && canonical(Object.keys(target).sort()) === canonical(['game', 'game_patch', 'realm'])
    && target.game === 'poe2' && target.game_patch === version
    && (target.realm === 'international' || target.realm === 'china'), 'Pack target mismatch');
  requireValue(locale === 'zh-CN' || locale === 'zh-TW', 'Unsupported pack locale');
  requireValue(strings(files) && files.length > 0 && new Set(files).size === files.length
    && files.every(filePath) && canonical(files) === canonical([...files].sort())
    && object(digests) && canonical(files) === canonical(Object.keys(digests).sort())
    && Object.values(digests).every(hex) && files.includes('manifest.json'), 'Invalid pack file inventory');
  requireValue(Array.isArray(blocks) && blocks.length === 3, 'Invalid runtime block list');
  const members: string[] = [];
  for (const [index, block] of blocks.entries()) {
    requireValue(object(block) && block.path === `blocks/${kinds[index]}.json`
      && Number.isSafeInteger(block.bytes) && (block.bytes as number) > 0 && hex(block.sha256)
      && strings(block.files) && canonical(block.files) === canonical([...block.files].sort())
      && block.files.every(name => filePath(name) && kindOf(name) === kinds[index]), 'Invalid runtime block specification');
    members.push(...block.files);
  }
  requireValue(new Set(members).size === members.length && canonical(members.sort()) === canonical(files), 'Block file inventory mismatch');
  const hashes = Object.fromEntries(Object.entries(digests).filter(([name]) => name !== 'manifest.json'));
  requireValue(hex(runtime_identity) && await digest(encoder.encode(canonical(['pobr-data-pack-v1', 3, version, hashes]))) === runtime_identity,
    'Runtime identity mismatch');
  const { release_id: omitted, ...payload } = value;
  void omitted;
  requireValue(hex(release_id) && await digest(encoder.encode(canonical(payload))) === release_id, 'Runtime release identity mismatch');
  return value as unknown as RuntimeRelease;
}

export async function decodeBlock(release: RuntimeRelease, spec: RuntimeBlockSpec, bytes: Uint8Array, digest: DigestFn): Promise<Map<string, Uint8Array>> {
  requireValue(bytes.length === spec.bytes && await digest(bytes) === spec.sha256, `Runtime block SHA-256 mismatch: ${spec.path}`);
  const block: unknown = JSON.parse(decoder.decode(bytes));
  requireValue(object(block) && block.schema === BLOCK_SCHEMA && canonical(block.target) === canonical(release.target)
    && block.kind === spec.path.slice('blocks/'.length, -'.json'.length)
    && object(block.files) && canonical(Object.keys(block.files).sort()) === canonical(spec.files), 'Mixed runtime block target/inventory');
  const files = new Map<string, Uint8Array>();
  for (const [name, text] of Object.entries(block.files)) {
    requireValue(typeof text === 'string', `Invalid runtime file: ${name}`);
    const raw = encoder.encode(text);
    requireValue(await digest(raw) === release.digests[name], `SHA-256 mismatch: ${name}`);
    // Validate syntax only. Stage the original string, never a JS reserialization.
    JSON.parse(text);
    files.set(name, raw);
  }
  return files;
}

export function validateSnapshot(release: RuntimeRelease, raw: Uint8Array): void {
  const snapshot: unknown = JSON.parse(decoder.decode(raw));
  const inventory = Object.fromEntries(Object.entries(release.digests)
    .filter(([name]) => name !== 'manifest.json' && !name.startsWith('overlay-common/')));
  requireValue(object(snapshot) && snapshot.schema_version === 3 && snapshot.poe_version === release.version
    && canonical(snapshot.languages) === canonical([release.locale]) && canonical(snapshot.files) === canonical(inventory),
  'Runtime snapshot identity/inventory mismatch');
  const domains = Array.isArray(snapshot.domains) ? { base: snapshot.domains } : snapshot.domains;
  requireValue(object(domains) && strings(domains.base), 'Invalid snapshot domains');
  for (const section of ['base', 'overlay', 'generated']) {
    const entries = domains[section] ?? [];
    requireValue(strings(entries) && entries.every(name => `${section}/${name}.json` in inventory), 'Missing snapshot domain');
  }
  requireValue(release.files.some(name => name.startsWith(`i18n/${release.locale}/`))
    && release.files.every(name => !name.startsWith('i18n/') || name.startsWith(`i18n/${release.locale}/`)), 'Mixed runtime locale');
}

/** Non-startup callers can recover one virtual file from its containing block. */
export async function fetchReleaseFile(release: RuntimeRelease, name: string,
  fetchBytes: (url: string) => Promise<Uint8Array>, digest: DigestFn): Promise<Uint8Array> {
  const spec = release.blocks.find(block => block.files.includes(name));
  requireValue(spec, `File not in runtime release: ${name}`);
  const files = await decodeBlock(release, spec, await fetchBytes(blockUrl(spec)), digest);
  return files.get(name)!;
}

export function blockUrl(spec: RuntimeBlockSpec): string {
  return `/data/h/${spec.sha256.slice(0, 16)}/${spec.path}`;
}
