# Website theming and colour mode

The pages embedded in `chatmail-www` (`www-src/`) carry a three-state colour mode and read a set of optional custom properties, so an operator can install a palette without the markup changing. `themes/` holds the palettes and the switcher that installs one into a served `www_dir`.

This covers the public website and the in-server documentation pages. The marketing site under `landing/` is a separate SvelteKit application with its own build; the two share only a storage-key convention.

## Operator guide

| Topic | Guide |
|-------|-------|
| Serving from a directory | [`html-export`](../guide/cli/html-export.md) · [`html-serve`](../guide/cli/html-serve.md) |
| Installing a palette | [`themes/README.md`](../../themes/README.md) |

## Colour mode

Three states, stored under `madmail-theme` in `localStorage`:

| State | Stored value | Appearance |
|-------|--------------|------------|
| Automatic | `system` | follows `prefers-color-scheme`, including changes while the page is open |
| Dark | `dark` | pinned |
| Light | `light` | pinned |

Resolution, highest first:

1. the stored value, if it is one of the three
2. the legacy `madmail-light` key, but only when it is exactly `true`, which becomes `light`
3. `--default-mode`, if the installed stylesheet declares one
4. `system`

Each step applies only when its condition holds. A legacy key with any value other than
`true`, or no legacy key at all, leaves the choice unset rather than resolving to `system`,
so an operator default still applies.

A visitor choosing automatic stores `system`, which outranks an operator default. Without that, a server pinning a mode would leave no way back to automatic.

`landing/src/lib/theme.svelte.js` uses the same state names and the same test on the legacy key, `madmail-light === 'true'` becoming `light`. What follows differs: landing returns `system`, having no operator default to defer to, where the embedded bootstrap returns nothing so that `--default-mode` can still apply. The two sites never share storage, since `localStorage` is origin-scoped, so this is consistency rather than interoperation.

### Applied state

Exactly one of `.dark` or `.light` is always set on the root element. The stock stylesheet scopes its media query to `:root:not(.light):not(.dark)`, so an explicit choice defeats the browser preference while the media query still governs with JavaScript disabled.

`data-mode` on the root element carries the **choice** — `system`, `dark` or `light` — because the classes carry the resolved appearance and cannot distinguish automatic-resolving-to-dark from pinned dark. The control's glyph keys off `data-mode`.

### The control

A button rather than a switch: a two-state control cannot honestly express three states.

| `data-mode` | Glyph | Label |
|-------------|-------|-------|
| `system` | `sun-moon` | Auto |
| `dark` | `moon` | Dark |
| `light` | `sun` | Light |

The glyph shows the **current** state rather than the next one. With two states the page itself tells a visitor where they are; with three it cannot, so the control is the only place that information can live. This differs deliberately from the admin panel's binary button, which advertises its destination.

The cycle runs `system` → `dark` → `light` → `system`, so the first click from automatic reaches the darker mode. A visitor on a bright page wanting relief gets it in one step, and the reverse journey is neither urgent nor uncomfortable.

Icons are Lucide (ISC), the maintained fork of Feather, which the admin panel already ships.

### Where the code lives

| Path | Role |
|------|------|
| `www-src/main.js` | the bootstrap, run at parse time before the body paints |
| `www-src/docker_docs.html`, `relay_docs.html`, `docs/fa/docker.html`, `docs/fa/relay.html` | inline copies; these four do not load `main.js` |
| `www-src/main.css` | the control's rules and the stock palette |

The four inline copies must stay byte-identical to the block in `main.js`.

## Palettes

A palette is one file, `themes/src/<name>.main.css`, carrying both palette blocks: `:root` for light and `:root.dark` for dark. The file name is the palette name, so there is no registry and adding one is dropping a file in `src/`.

Palettes replace `main.css` wholesale rather than layering over it, so any rule the site needs must exist in every palette as well as in the stock stylesheet.

### The switcher

`themes/switch <palette> [--default-mode light|dark]` copies the source into `$MADMAIL_WWW/main.css`. The flag is optional and unset by default; it inserts one declaration into `:root`.

Install and verification share one `emit()`, so the served file and the expected file cannot be produced by different logic.

Verification **reproduces** rather than fingerprints: it regenerates what the install would write and compares. A recorded hash would catch edits to the served tree but not drift in `src/`, which inverts what the check exists for. A palette difference and a mode difference are reported distinguishably, and the file is named.

Installation writes through `mktemp(1)` in the destination directory and renames, so it cannot follow a symlink and a failed write leaves the previous stylesheet intact.

Two guards refuse before anything is written: a value left as an unfilled `__…__` placeholder, and a palette that declares `--default-mode` itself, which would fight the flag.

### Recovery

The switcher only ever writes `main.css` inside the served tree, so restoring the stock appearance is a matter of putting the exported `main.css` back.

## Tests

| Coverage | Location |
|----------|----------|
| Every page declares a favicon and carries the control | `crates/chatmail-www/src/tests.rs` |
| Pages carrying the icons carry their licence notices | `crates/chatmail-www/src/tests.rs` |
| `logo.svg` begins with its XML declaration | `crates/chatmail-www/src/tests.rs` |
| Switcher install, verification, destination and name guards, temporary files | `scripts/test_theme_switch.py` |
| The bootstrap is identical in all five copies | `crates/chatmail-www/src/tests.rs` |
| The bootstrap applies both `.dark` and `.light` | `crates/chatmail-www/src/tests.rs` |
| Mode resolution, cycling, persistence and a live preference change | `scripts/colour_mode_test.mjs` |

**Not covered:** the bootstrap runs against stub globals rather than a browser, so what is exercised is its logic and not paint timing or a real `matchMedia`. The four inline copies are not executed separately; the test slices the block out of `main.js` and a Rust test pins the copies as byte-identical to it, so their behaviour is covered transitively rather than directly.

See [16-testing.md](16-testing.md).

## Related

- [13-configuration.md](13-configuration.md) — `www_dir` and setting precedence
- [26-configuration-precedence.md](26-configuration-precedence.md) — how `--default-mode` relates to the wider precedence rules
