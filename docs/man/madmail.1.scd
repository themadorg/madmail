madmail(1) "@VERSION@" "User Commands"

# NAME

madmail \- Chatmail mail server and operator CLI

# SYNOPSIS

*madmail* [*options*]

*madmail* *run* [*options*]

*madmail* *command* [*subcommand*] [*options*] [*arguments*...]

Common forms:

*madmail* [*--config* _file_] [*--state-dir* _dir_] [*--json*]

*madmail* *install* *--simple* *--ip* _address_ [_options_...]

*madmail* *accounts* *status*

*madmail* *admin-token* [*--raw*]

# DESCRIPTION

*madmail* is a single executable that runs a Chatmail mail server (SMTP,
submission, IMAP, HTTP registration UI, admin API) and provides operator
commands compatible with the Madmail and maddy CLIs.

Invoking *madmail* with no subcommand is equivalent to *madmail run* and
starts network services defined in the configuration file.

Operator subcommands open the configuration and state databases directly
or call the admin API.
They are intended for installation, monitoring, and day\-to\-day
administration on the host where the server runs.

The executable basename (for example *madmail* or *chatmail*) is taken
from *argv[0]*.
It determines default config and state paths, the systemd unit name, the
service user name, and installed man/completion file names when using
*install*.

Extended documentation:
<https://github.com/themadorg/madmail/tree/main/docs/guide/cli>

## Server mode

*run*
	Start SMTP, submission, IMAP, HTTP, and related endpoints.
	Read configuration from *\-\-config* and persistent state from
	*\-\-state\-dir* (alias *\-\-libexec*).

## Installation and lifecycle

*install*
	Write configuration, TLS material, databases, optional systemd unit,
	service user, system\-wide binary, manual page, and shell tab
	completion scripts.
	See EXAMPLES and NOTES.

*uninstall*
	Remove the service, configuration, and data.
	Confirmation is required; use keep flags such as *--keep-data* to
	preserve selected artifacts.

*upgrade*, *update*
	Replace this executable from a signed local file or URL.
	After replace, may prompt to convert custom *www_dir* Go templates
	to Minijinja (see *html\-migrate*).

*reload*
	Apply database configuration overrides and restart via the admin API.

*version*
	Print package version and exit.

*status*
	Show server status (connections, registered users, uptime).

## Accounts and access

*accounts*
	Account management: *status*, *info*, *create*, *create\-random*,
	*delete*, *ban*, *unban*, *export*, *import*, *delete\-all*.

*ban\-list*
	List blocklisted usernames (alias for *accounts ban\-list*).

*blocklist*
	Manage usernames blocked from re\-registration.

*create\-user*
	Create a random account and print JSON credentials.

*delete*
	Fully delete a user (credentials, mail, blocklist entry).

*admin\-token*
	Display admin API credentials (optional login QR code).

*admin\-web*
	Enable, disable, or configure the embedded admin web dashboard.

*registration*
	Open or close public account registration.

*registration\-tokens* (*reg\-tokens*, *tokens*)
	Manage invite tokens for account creation.

## Mail policy and delivery

*federation*
	Federation policy, domain rules, and silent\-dismiss lists.

*endpoint\-cache* (*dns\-cache*)
	Outbound delivery DNS override cache.

*sharing*
	Delta Chat contact share links.

*message\-size*
	View or set maximum message size limits.

*tasks*
	Scheduled maintenance jobs (message retention, unused accounts, queue
	purge).

*queue*
	Delivery queue management.

## Network and TLS

*port*
	Admin\-panel listener ports and local/public bind mode.

*certificate*
	TLS certificates (Let's Encrypt autocert, file, self\-signed).

*submission\-access*
	SMTP submission access scope (public or local).

*push*
	Delta Chat push notification mode (*auto*, *on*, *off*).

*proxy* (*pr*)
	Shadowsocks circumvention proxy (*status*, *enable*, *disable*,
	*cipher*, *password*).
	Requires *ss_addr* and *ss_password* in the configuration file.
	Port overrides use *port shadowsocks*.
	Run *reload* after changes to apply to a running server.

*webimap*, *websmtp*
	Enable or disable WebIMAP and WebSMTP HTTP APIs.

## Customization

*language*
	Website language (*en*, *fa*, *ru*, *es*).

*html\-export*
	Export default HTML/CSS assets to a directory.

*html\-serve*
	Serve the web UI from an external directory (*embedded* reverts to
	built\-in files).

*html\-migrate*
	Convert custom *www_dir* HTML from Go *html/template* syntax to
	Minijinja (interactive; *--yes* for scripts).

*hash*
	Generate password hashes for *pass_table* configuration.

## Shell documentation helpers

*completion* *bash*|*zsh*|*fish*
	Print a tab\-completion script to stdout.

Hidden packager helpers (not shown in default help):
*generate\-man*, *generate\-fish\-completion*.

# OPTIONS

The following options are accepted before every subcommand.

*\-h*, *\-\-help*
	Print a short usage summary and exit.

*\-\-config* _file_
	Configuration file path.
	Overrides the built\-in default when set.
	Environment variable: *CHATMAIL_CONFIG*.
	Default: _/etc/madmail/madmail.conf_ (or a development default when
	that file is absent).

*\-\-state\-dir* _dir_, *\-\-libexec* _dir_
	Persistent state directory (databases, mail storage, ACME state,
	queues).
	Environment variable: *CHATMAIL_STATE_DIR*.
	Default: _/var/lib/madmail_ (or a development default under the
	current working directory).

*\-\-json*
	Emit machine\-readable JSON on standard output for operator
	subcommands.
	Suppresses decorative text and QR codes.
	On failure, a JSON error object is written to standard error and the
	exit status is non\-zero.

Subcommands accept additional options documented in *madmail command
\-\-help* and in the online CLI reference.

# EXIT STATUS

*0*
	Success.

*1*
	General error (configuration, I/O, validation, or remote admin API
	failure).
	An explanatory message is printed to standard error.

# ENVIRONMENT

*CHATMAIL_CONFIG*
	Default configuration file path.
	See *\-\-config*.

*CHATMAIL_STATE_DIR*
	Default state directory.
	See *\-\-state\-dir* and *\-\-libexec*.

# FILES

_/etc/madmail/madmail.conf_
	Default main configuration file after system install (actual name is
	/etc/madmail/BINARY.conf where BINARY is the executable basename).

_/etc/madmail/certs/_
	Default TLS certificate and private key directory after install.

_/var/lib/madmail/_
	Default state directory after system install (actual path uses the
	executable basename).

_/usr/local/bin/madmail_
	Default destination for the binary on system install.

_/usr/share/man/man1/madmail.1_
	Manual page installed by *madmail install* on system installs.

_/usr/share/bash-completion/completions/madmail_
	Bash tab completion installed by *madmail install*.

Zsh tab completion is installed under /usr/share/zsh/site-functions/ using
the executable basename prefixed by an underscore (for example
_/usr/share/zsh/site-functions/madmail_ when the binary is *madmail*).
Installed by *madmail install*.

_/usr/share/fish/vendor_completions.d/madmail.fish_
	Fish tab completion installed by *madmail install*.

_/etc/systemd/system/madmail.service_
	Default systemd unit path after system install.

# EXAMPLES

Simple install on a public IPv4 address (self\-signed TLS by default):

	sudo madmail install --simple --ip 203.0.113.50 --lang en

Install on a DNS hostname with Let's Encrypt (port 80 must be free during
install when certificate issuance runs):

	sudo madmail install --simple --domain mail.example.org --acme-email admin@example.org --lang en

Preview a system install without writing files:

	sudo madmail install --simple --ip 203.0.113.50 --dry-run

Start the server under systemd after install:

	sudo systemctl enable --now madmail
	journalctl -u madmail -n 100 --no-pager

Start manually with explicit paths (local or development layout):

	madmail --config /etc/madmail/madmail.conf run --libexec /var/lib/madmail

Show account summary as JSON for scripting:

	madmail --json accounts status

Print admin API token for scripts:

	TOKEN=$(madmail admin-token --raw --no-qr)

Install bash completion without a full system install:

	madmail completion bash | sudo tee /usr/share/bash-completion/completions/madmail

Enable Shadowsocks and show the client URL:

	madmail proxy enable
	madmail proxy status
	madmail reload

# NOTES

*install* requires root for system installs (service user, binary copy,
systemd unit, man page, and completions).
Use explicit non\-FHS *\-\-config\-dir* and *\-\-state\-dir* for local
installs that skip those steps.

Interactive *install* (without *\-\-simple* or *\-\-non\-interactive*) may
not be available in all builds; use *\-\-simple* or *\-\-non\-interactive*
in scripts.

When *\-\-json* is set, successful operator commands print a single JSON
object on standard output.
See the online JSON schema guide in the repository docs/guide/cli/
directory.

# CAVEATS

TLS with a public IP relay often uses self\-signed certificates or
*turn_off_tls* unless *\-\-auto\-ip\-cert* and a valid ACME contact email
are supplied.
Delta Chat clients may need to accept self\-signed certificates on such
relays.

Replacing the running binary in place while the server is active is
unsafe.
Use *upgrade* or stop the service before overwriting the executable.

# BUGS

Report issues at
<https://github.com/themadorg/madmail/issues>.

# SEE ALSO

*systemctl*(1),
*journalctl*(1),
*mandb*(8)

Online CLI reference:
<https://github.com/themadorg/madmail/tree/main/docs/guide/cli>

Install guide:
<https://github.com/themadorg/madmail/tree/main/docs/guide/install.md>