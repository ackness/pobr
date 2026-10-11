# Iteration log

## 1 — Survey and populated passive-route verification

- Chose the long-term goal of trustworthy build decisions and the first milestone of passive reallocation integrity. Surveyed product/docs, current planner and tests, recent history and open issues (none returned). Recorded auto focus; left the paused `/goal` untouched.
- Built this branch's release WASM and ran the actual web app on port 5182. Imported the committed level-98 Frost Bomb fixture, searched a three-point route and visually inspected its tree preview and limits.
- Search evaluated 391 plans. Search/preview did not mutate saved state. Apply added exactly nodes 17796/12412/45497 (131 to 134 total), preserved every other state field and the other saved build, and produced DPS 7560.833856375 against preview 7560.83.
- Reload verification passed semantic deep equality on a fresh repeated flow. The initial raw-JSON comparison failure was object-key ordering, not a persistence defect.
- Refund search was correctly unavailable for the imported build's 23/22 weapon-exclusive nodes, with a visible explanation. Positive refunds, changed travel attributes, stale plans and excluded edits remain milestone work.
- No production-code changes or permanent tests were justified by this investigation. No full test/parity pass is claimed. Branch-local build, live WASM calculations, UI actions and screenshots are the evidence.
- Removed temporary screenshots and finished browser space 29. Retained the Vite service for the next iteration. Commit: this entry's checkpoint commit (`chore: record populated passive route verification`).
- Next: a connected positive refund/reallocation fixture with a replacement travel attribute, protected sockets and exact preview/apply/reload checks.