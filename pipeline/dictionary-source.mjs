// Cache complete dictionary snapshots by immutable upstream commit.
import fs from 'node:fs/promises';
import path from 'node:path';

export async function dictionarySource({ cacheRoot, files, ref, refresh = false, fetcher = fetch }) {
  if (ref && !/^[a-f0-9]{40}$/.test(ref)) throw new Error('Dictionary ref must be a full commit SHA.');
  if (refresh || !ref) {
    const response = await fetcher('https://api.github.com/repos/addohm/poe2-en-cn-dict/commits/HEAD', {
      headers: { accept: 'application/vnd.github+json' }, signal: AbortSignal.timeout(15000),
    });
    if (!response.ok) throw new Error(`Dictionary commit lookup: HTTP ${response.status}`);
    ref = (await response.json()).sha;
    if (!/^[a-f0-9]{40}$/.test(ref ?? '')) throw new Error('Invalid dictionary commit response.');
  }
  const directory = path.join(cacheRoot, ref);
  const complete = await Promise.all(files.map(file => fs.access(path.join(directory, file)).then(() => true, () => false)));
  if (complete.every(Boolean)) return { directory, ref };
  await fs.mkdir(cacheRoot, { recursive: true });
  const temporary = await fs.mkdtemp(path.join(cacheRoot, '.download-'));
  try {
    for (const file of files) {
      const response = await fetcher(`https://raw.githubusercontent.com/addohm/poe2-en-cn-dict/${ref}/dictionary/${file}`, {
        signal: AbortSignal.timeout(30000),
      });
      if (!response.ok) throw new Error(`Dictionary ${file}: HTTP ${response.status}`);
      const text = await response.text();
      JSON.parse(text);
      const destination = path.join(temporary, file);
      await fs.mkdir(path.dirname(destination), { recursive: true });
      await fs.writeFile(destination, text, 'utf8');
    }
    // An incomplete existing snapshot is an error; never overwrite cached inputs.
    await fs.rename(temporary, directory);
  } finally {
    await fs.rm(temporary, { recursive: true, force: true });
  }
  return { directory, ref };
}
