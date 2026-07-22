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
- [ ] `madmail-tray --smoke-exit` exits 0; tray menu opens admin / start-stop works
- [ ] Admin token file under `%ProgramData%\Madmail\data\admin_token`
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
