const SHARE_HOSTS = new Set([
  'pobb.in',
  'poe.ninja',
  'poe2.ninja',
  'pastebin.com',
  'pastebinp.com',
  'rentry.co',
  'poe2db.tw',
  'maxroll.gg',
  'www.wegame.com.cn',
]);

/** Whether pasted text is a build someone shared: a PoB code, or a link to a site that hosts them. */
export function looksLikeBuild(text: string): boolean {
  const value = text.trim();
  if (!value) return false;
  if (/^https:\/\//i.test(value) && !/\s/.test(value)) {
    try {
      const url = new URL(value);
      return url.protocol === 'https:' && SHARE_HOSTS.has(url.hostname);
    } catch {
      return false;
    }
  }
  return value.length >= 80 && /^[A-Za-z0-9_\-+/=]+$/.test(value);
}
