# Windows release checklist (L8)

Run before claiming a public Windows setup.exe or closing epic [#103](https://github.com/themadorg/madmail/issues/103) via a `main` merge.

## Environment

- [ ] Windows 10/11 x64 machine (or VM)
- [ ] Windows 11 arm64 machine or lab (if shipping arm64)
- [ ] Built or CI artifacts: `madmail.exe`, `madmail-tray.exe`, optional `*-setup.exe`

## Installer / CLI

- [ ] **Local self-signed:** setup wizard or  
  `madmail install --simple --ip 127.0.0.1 --tls-mode self_signed --install-service --start-service --firewall`
- [ ] Service shows **Running** (`madmail service status` / Services MMC)
- [ ] Firewall rules present (`Madmail (*)` in Windows Firewall)
- [ ] `madmail-tray --smoke-exit` exits 0; tray start/stop service and “copy admin token” work
- [ ] Admin token file under `%ProgramData%\Madmail\data\admin_token` (use CLI / `POST /api/admin` — Windows builds do not embed the admin-web SPA)
- [ ] Service install does **not** fail with sc exit **1639** (uses Win32 CreateService, not quoted `sc start=`)
- [ ] **Full mail path** (or `vagrant` E2E): create two users, SMTP 587 → IMAP 143 both directions — see [vagrant/README.md](./vagrant/README.md)
- [ ] **Public IP self-signed** (optional lab)
- [ ] **Domain LE** or **auto-IP cert** when port 80 and DNS/IP allow (optional)

## Upgrade / uninstall

- [ ] Re-run setup or replace binaries without wiping mail (`--keep-data` path)
- [ ] Uninstall keeps data when expected; full wipe when requested

## Docs smoke

- [ ] Quick-start Windows section matches current flags
- [ ] `packaging/windows/README.md` build steps work on a clean checkout

## Sign-off

| Arch | Tester | Date | Notes |
|------|--------|------|--------|
| amd64 | | | |
| arm64 | | | |
