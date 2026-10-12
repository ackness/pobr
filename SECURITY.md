# Security policy

## Supported versions

Only the latest release and the `master` branch receive fixes.

## Reporting a vulnerability

Report privately through
[GitHub security advisories](https://github.com/ackness/pobr/security/advisories/new).
Do not open a public issue for a vulnerability.

Include what is affected (the web app, the Pages Worker, the CLI, the WASM
package or the data pipeline), the steps to reproduce it and the impact you
expect. You should get a reply within a week.

## Scope

PoBR calculates in the browser. The Pages Worker only adapts a restricted set
of HTTP requests for build import and market data. Reports about imported
build codes or saved workspaces that crash, hang or run script in the page,
and about requests the Worker should not forward, are in scope. Wrong
calculation results are ordinary bugs; report them as issues.
