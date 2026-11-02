# Portable source envelopes

The implementation, source processing and regression tests now belong to the
independent RePoE repository. See the [pinned upstream contract](https://github.com/ackness/repoe/blob/a27c0a795728bb64d988519ccebe7faeacf9edf7/RePoE/docs/source-rebinding.md).
PoBR has no local acquisition or envelope-creation implementation.

Run in the independent RePoE checkout:

```sh
mise run source:rebind -- --help
# Equivalent qualified package entry point:
python -B -m RePoE.source_rebinding --help
```

The nested `ordinary-input.zip` and authenticated handoff remain active inside
the multi-owner ordinary input package. PoBR must verify that envelope; it does
not recreate the upstream proof or replace it with an outer ZIP hash.

[Ordinary input preflight](../runtime/ordinary/ordinary-inputs.md) acquires exactly three pinned RePoE
files (`__init__.py`, `source_io.py`, `source_rebinding.py`) into a temporary build
directory. It calls `verify(archive, authenticated_handoff, absent_output)` through
the [pinned loader](source_verifier.py), which has no acquisition-module import
fallback. Generic local path/hash checks and game interpretation stay in PoBR.

Historical `pobr-*` protocol names and original receipts keep their identity.
New tool fingerprints produce new receipts. Acquisition consistency, physical
decoding, [bounded source decisions](client-source-draft.md), gameplay semantics
and runtime adoption remain separate; no command here switches CURRENT.
