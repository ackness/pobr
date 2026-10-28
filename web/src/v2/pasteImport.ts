const SHARE_HOSTS = /^(?:[\w-]+\.)*(?:pobb\.in|poe\.ninja|pastebin\.com|rentry\.(?:co|org)|poe2db\.tw|maxroll\.gg|wegame\.com\.cn|pathofexile\.com)$/i;

/** Whether pasted text is a build someone shared: a PoB code, or a link to a site that hosts them. */
export function looksLikeBuild(text: string): boolean {
  const value = text.trim();
  if (!value) return false;
  if (/^https?:\/\//i.test(value) && !/\s/.test(value)) {
    try { return SHARE_HOSTS.test(new URL(value).hostname); } catch { return false; }
  }
  return value.length >= 80 && /^[A-Za-z0-9_\-+/=]+$/.test(value);
}
