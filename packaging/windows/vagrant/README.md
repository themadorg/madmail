# Vagrant Windows E2E (libvirt)

Local **release-style** testing of Madmail on Windows: install, Windows service, firewall, tray smoke, and a **full mail path** (create users → SMTP submit → IMAP receive, both directions).

| Layer | Role |
|-------|------|
| **GitHub Actions** (`windows.yml`) | Always-on CI smoke (no full mail path required) |
| **This Vagrant box** | Optional local gate before calling a release “good on Windows” |

Does **not** publish releases or replace CI.

## What it tests

1. Copy host-built `madmail.exe` (+ optional tray) into the guest  
2. `madmail install --simple --ip 127.0.0.1 --tls-mode self_signed --install-service --start-service --firewall`  
3. `madmail service status`  
4. `madmail-tray --smoke-exit` (if tray binary present)  
5. **Python E2E** (`e2e/mail_e2e.py`):
   - `accounts create` for alice + bob  
   - SMTP AUTH + STARTTLS on **587** (self-signed accepted)  
   - IMAP STARTTLS on **143** — wait until bob has the message  
   - Reverse: bob → alice  

Not covered here (manual / RDP): interactive tray menu, Inno GUI wizard clicks, Delta Chat desktop.

## Host prerequisites

- Linux host with **KVM/libvirt**, `vagrant`, **`vagrant-libvirt`**
- Nested virtualization strongly recommended for Windows guests  
- Disk space for a Windows box (tens of GB)

```bash
# Debian/Ubuntu-ish example
sudo apt install vagrant qemu-kvm libvirt-daemon-system
vagrant plugin install vagrant-libvirt
```

### Windows box (Microsoft evaluation)

Microsoft does not ship a first-party box on Vagrant Cloud for every product. Recommended approaches:

1. **Preferred:** Build or obtain a box from a **Microsoft Evaluation Center** ISO (Windows Server 2022 / Windows 11 Enterprise eval), then:
   ```bash
   vagrant box add madmail/windows-eval /path/to/your.box
   export MADMAIL_WIN_BOX=madmail/windows-eval
   ```
2. **Convenient default in `Vagrantfile`:** `gusztavvargadr/windows-server-2022-standard`  
   Community box commonly used for WinRM + libvirt; still subject to Microsoft eval EULA when based on eval media. **Pin a version** for reproducibility:
   ```bash
   export MADMAIL_WIN_BOX_VERSION='...'   # see `vagrant box list -i`
   ```

Override credentials if your box differs:

```bash
export MADMAIL_WIN_USER=vagrant
export MADMAIL_WIN_PASSWORD=vagrant
```

## Host: produce binaries

From the **repo root**:

```bash
make build-windows-amd64
# ensure:
ls -la build/madmail-windows-amd64.exe
# optional:
ls -la build/madmail-tray-windows-amd64.exe
```

Or drop CI artifacts into `build/` with those names.

## Run

```bash
cd packaging/windows/vagrant
vagrant up --provider=libvirt
```

First boot provisions:

| Step | Script |
|------|--------|
| Bootstrap | `provision/01-bootstrap.ps1` (Chocolatey + Python) |
| Install | `provision/02-install-madmail.ps1` |
| E2E | `provision/03-e2e-mail.ps1` (`run: always`) |

Re-run E2E after code/binary updates:

```bash
# rebuild on host, then:
vagrant rsync
vagrant provision --provision-with shell   # or destroy/up
# E2E only (03 is run: always on provision):
vagrant winrm -e -- powershell -File C:\vagrant\provision\03-e2e-mail.ps1
```

Destroy:

```bash
vagrant destroy -f
```

## Manual extras (RDP)

```bash
# If SPICE/RDP is configured for the domain:
virsh domdisplay madmail-win   # name may vary
```

Then exercise tray UI and Inno `setup.exe` if copied into the guest.

## Environment variables

| Variable | Default | Meaning |
|----------|---------|---------|
| `MADMAIL_WIN_BOX` | `gusztavvargadr/windows-server-2022-standard` | Vagrant box name |
| `MADMAIL_WIN_BOX_VERSION` | (unset) | Pin box version |
| `MADMAIL_WIN_MEMORY` | `4096` | MiB |
| `MADMAIL_WIN_CPUS` | `2` | vCPUs |
| `MADMAIL_WIN_USER` / `_PASSWORD` | `vagrant` | WinRM |

## Layout inside guest

| Path | Role |
|------|------|
| `C:\Program Files\Madmail\madmail.exe` | Server + CLI |
| `C:\ProgramData\Madmail\config\` | Config + certs |
| `C:\ProgramData\Madmail\data\` | DB, mail, admin_token |
| `C:\vagrant\e2e\mail_e2e.py` | Mail-path harness |
| `C:\madmail-build\` | Host `build/` rsync |

## Troubleshooting

| Symptom | What to try |
|---------|-------------|
| WinRM timeout on first boot | Increase `config.vm.boot_timeout`; ensure box supports WinRM |
| No binary found | Host `build/madmail-windows-amd64.exe` missing; re-rsync |
| SMTP/IMAP connection refused | `madmail service status`; wait longer after install |
| TLS errors in Python | E2E disables cert verify for self-signed; ensure STARTTLS ports 587/143 |
| libvirt disk/driver issues | Prefer boxes with VirtIO/SATA as in this Vagrantfile |

## Relation to epic #103

This is **local polish** for release confidence (full mail functionality on Windows), complementing:

- GHA Windows smoke (install + certs + tray smoke)  
- [MANUAL-CHECKLIST.md](../MANUAL-CHECKLIST.md) for human/RDP steps  
