# Client source acquisition

The implementation, source processing and regression tests now belong to the
independent RePoE repository. See the [pinned upstream contract](https://github.com/ackness/repoe/blob/6b65ade6ac8731515b28aac2aa3c38f160d0bffd/RePoE/docs/client-sources.md).
PoBR has no local acquisition or envelope-creation implementation.

Run in the independent RePoE checkout:

```sh
mise run client:source -- --help
# Equivalent qualified package entry point:
python -B -m RePoE.client_sources --help
```

Historical `pobr-*` protocol names and original receipts keep their identity.
New tool fingerprints produce new receipts. Acquisition consistency, physical
decoding, [bounded source decisions](client-source-draft.md), gameplay semantics
and runtime adoption remain separate; no command here switches CURRENT.
