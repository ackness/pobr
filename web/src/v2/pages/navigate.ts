import { createContext, useContext } from 'react';
import { routePath, type Route } from '../routes';

/** The shell's `navigate`, offered to panels that need to move to another page. */
export const V2NavigateContext = createContext<((route: Route) => void) | null>(null);

/** Move to another v2 page from inside a panel, through the shell so the page transition runs. */
export function useV2Navigate(): (route: Route) => void {
  const navigate = useContext(V2NavigateContext);
  // Without the shell (a fixture, a test) fall back to the history the shell listens to.
  return navigate ?? ((route) => {
    const path = routePath(route);
    if (location.pathname !== path) history.pushState(null, '', path);
    dispatchEvent(new PopStateEvent('popstate'));
  });
}
