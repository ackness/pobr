# Fixed client index discovery

The implementation, source processing and regression tests now belong to the
independent RePoE repository. See the [pinned upstream contract](https://github.com/ackness/repoe/blob/6b65ade6ac8731515b28aac2aa3c38f160d0bffd/RePoE/docs/client-index.md).
PoBR has no local acquisition or envelope-creation implementation.

Run in the independent RePoE checkout:

```sh
mise run client:locate -- --help
# Equivalent qualified package entry point:
python -B -m RePoE.client_index --help
```

Default discovery remains strict: any missing resource fails without a successful
selection or index receipt. The explicit upstream `--diagnostic` mode preserves
matched records, missing paths and framing evidence; missing results exit 2.
Inspection never creates a successful selection, even when all paths match.
A smaller successful subset needs its own explicit request.

Historical `pobr-*` protocol names and original receipts keep their identity.
New tool fingerprints produce new receipts. Acquisition consistency, physical
decoding, [bounded source decisions](client-source-draft.md), gameplay semantics
and runtime adoption remain separate; no command here switches CURRENT.
