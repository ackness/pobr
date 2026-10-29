/** Workflow order: read the result, change what it is built from, then improve and verify it. */
export const NAV_GROUPS = [
  ['overview'],
  ['skills', 'items', 'tree', 'config'],
  ['trade', 'compare', 'calcs'],
] as const;
/** Importing and managing builds is an action, not a stage of the workflow, so it has no tab. */
export const ROUTES = [...NAV_GROUPS.flat(), 'build'] as const;
export type Route = typeof ROUTES[number];
export const DEFAULT_ROUTE: Route = 'overview';
export function readRoute(path: string): Route {
  const page = path.replace(/^\//, '').replace(/\/$/, '');
  return ROUTES.includes(page as Route) ? page as Route : DEFAULT_ROUTE;
}
export function routePath(route: Route) { return route === DEFAULT_ROUTE ? '/' : `/${route}`; }
