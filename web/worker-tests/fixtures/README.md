Build-share fixtures

The fixed URL mappings follow the pinned PoB2 BuildSiteTools raw-code adapters.
All six return the recorded `build-code.txt` response body in these offline tests.
That code is a synthetic PoB2 XML build with two loadouts and a non-default active
effect, compressed once for this fixture. It contains no account data and is not
a claim that these fixture URLs exist on the live services. No live service is
used by the tests; HTML, missing-build and oversized responses are separate cases.
