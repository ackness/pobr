export const ROUTES = ['overview', 'build', 'tree', 'skills', 'items', 'trade', 'calcs', 'config', 'compare'] as const;
export type Route = typeof ROUTES[number];
export function isV2Path(path: string) { return /^\/v2(?:\/|$)/.test(path); }
export function readRoute(path: string): Route {
  const page = path.replace(/^\/v2\/?/, '').replace(/\/$/, '');
  return ROUTES.includes(page as Route) ? page as Route : 'overview';
}
export function routePath(route: Route) { return route === 'overview' ? '/v2' : `/v2/${route}`; }
