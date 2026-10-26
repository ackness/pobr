import { isV2Path } from './v2/routes';

// Load one visual entry point; legacy global styles never outrank V2 tokens.
void (isV2Path(location.pathname) ? import('./v2/entry') : import('./legacy-entry'));
