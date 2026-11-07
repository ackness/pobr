/**
 * Address of a Playwright config's web server. `mise run e2e` passes a free port derived from the worktree path in
 * POBR_E2E_PORT (devs/scripts/e2e.py); a plain `playwright test` uses the config's fixed port. Servers bind
 * 127.0.0.1 with --strictPort, so a port another worktree already uses fails the run instead of testing its build.
 */
export function e2eServer(fallbackPort: number) {
  const port = Number(process.env.POBR_E2E_PORT ?? fallbackPort);
  return { port, baseURL: `http://127.0.0.1:${port}` };
}
