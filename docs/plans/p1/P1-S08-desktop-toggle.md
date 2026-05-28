# P1-S08: deltachat-desktop — Advanced settings toggle

## Action

In `context/deltachat-desktop`:

1. Add `packages/frontend/src/components/Settings/WebimapTransport.tsx` (mirror `WebxdcRealtime.tsx`):
   - `getConfig` / `setConfig`: `webimap_transport_enabled`
   - Default off when unset (`0`)
2. Mount in `Advanced.tsx` when `is_chatmail === '1'`.
3. `data-testid='webimap-transport-switch'`.
4. Optional button → Connectivity dialog.

## Files touched

- `packages/frontend/src/components/Settings/WebimapTransport.tsx`
- `packages/frontend/src/components/Settings/Advanced.tsx`
- `packages/shared/locales/_untranslated_en.json` (strings)

## Tests (implement with this step)

| Test ID | Tier | Location | Asserts |
|---------|------|----------|---------|
| **P1-UI01** | Manual | Desktop | Toggle on → `getConfig` returns `"1"`; off → `"0"`; only visible for chatmail account |
| **P1-UI01b** | Manual | Desktop | After enable, Connectivity shows WebIMAP line (depends on P1-S07) |
| **P1-E2E-UI** | E2E (optional) | `packages/e2e-tests/` | Playwright: open Advanced → flip switch → assert RPC mock or real core config — **stretch goal** |

### P1-UI01 checklist

1. Configure chatmail test account on desktop.
2. Settings → Advanced → enable **WebIMAP transport (experimental)**.
3. Restart or wait for scheduler — Connectivity shows WebIMAP connected (server flags on).
4. Disable toggle — Core stops WS (Connectivity shows transport off).
5. Non-chatmail account: switch **not** shown.

### P1-UI01b

1. With server `webimap disable`, enable client toggle → Connectivity warning, IMAP still works.

## Verification

```bash
# Build desktop (smoke)
cd context/deltachat-desktop && pnpm build:electron  # or project’s usual target

# Optional playwright (if implemented)
cd packages/e2e-tests && pnpm test -- webimap-transport
```

**Step done when:** P1-UI01 + P1-UI01b signed off in PR description; screenshot optional.

## Linked tests

| Test ID | Step |
|---------|------|
| P1-UI01 | P1-S08 |
| P1-UI01b | P1-S08 |
| P1-E2E-UI | P1-S08 (optional) |

## Next

[P1-S09-e2e-core-chatmail.md](P1-S09-e2e-core-chatmail.md)
