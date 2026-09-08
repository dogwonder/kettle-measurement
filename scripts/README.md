# scripts/

Packaging scripts, in the order they run. The packaging sequence needs
gitignored assets or a real build and runs locally.

## Scaffold a Mini

From Kettle's root:

```sh
./scripts/new-mini image-descriptions
cd ../kettle-mini-image-descriptions
npm run setup
npm start
```

For appointment letters, run from Kettle's root:

```sh
./scripts/new-mini appointments
cd ../kettle-mini-appointments
npm run setup
npm start
```

`new-mini` creates the sibling directory and refuses to overwrite anything.
`--output /path/to/new-directory` chooses another destination whose parent exists.
It requires Python 3. Image scaffolding also needs Kettle's installed app
dependencies to compile its GOV.UK styles. Appointments must stay beside Kettle
and compiles those styles during setup. Scaffolding downloads nothing. Image
setup installs a Python environment and pinned GIT model; appointment setup
installs Node dependencies and builds against Kettle's reader and existing model.

The scaffold instructions, source template and task-specific README live in
[`app/mini-templates/image-descriptions/`](../app/mini-templates/image-descriptions/README.md)
and [`app/mini-templates/appointments/`](../app/mini-templates/appointments/README.md).
The command and these instructions are in the published `scripts/` tree; the
product template stays under private `app/`. The public measurement projection
alone cannot generate this UI, and the command explains the missing prerequisite.
The task selects its runtime automatically; `--runtime` optionally validates it.
Only these two task/runtime pairs are supported. Templates are independent
copies; the image Mini runs independently of Kettle, while Appointments reuses
its runner and model. Caption logic stays in the image Mini. Its JSON
CLI is the potential integration boundary if Kettle gains a concrete consumer;
captions must not be treated as verified source readings.

Check generator refusal and standalone output with
`python3 -m unittest discover -s scripts -p 'test_new_mini.py'`. The generated
project carries its own input, worker and local-server tests.

From either generated Mini, use `npm run setup`, `npm start`, `npm test`, `npm run check` and
`npm run test:browser`. The shared [common template](../app/mini-templates/README.md)
owns the setup/start entry points and Playwright configuration. Setup installs
the Node developer tools as well as preparing the task runtime. Browser tests
start the local server automatically and use installed Chrome on macOS, or
Chromium installed with `npm run test:browser:install`. The shell entry points
`./scripts/setup.sh` and `./scripts/start.sh` remain available.

| Directory beside Kettle | Local address | Model |
| --- | --- | --- |
| `kettle-mini-appointments/` | `http://127.0.0.1:8787` | Reuses Kettle's installed Qwen3.5-4B and reader. |
| `kettle-mini-image-descriptions/` | `http://127.0.0.1:8788` | Own pinned GIT model, downloaded during setup. |

Appointments preserves the original Mini's reading and payment-reminder fixes.
Its setup installs dependencies and builds the app without copying or downloading
a model. Both use the retained lowercase
`reference/logo/kttl-lowercase-spout-right-on-navy.svg` wordmark.

## Other scripts

`capability-coverage.py` reports the inventory's executable checks and
their limits without running a model. Its claim checks run in CI:
`python3 -m unittest discover -s scripts -p 'test_capability_coverage.py'`.
See [the capability inventory](../evals/capabilities/README.md) for the
owning Rust tests and the distinction between a check and a capability.

```
vendor-sidecar.sh   →   sign-macos.sh   →   tauri build   →   smoke-install.sh
   the engine            the signature       the bundle        does it stand up
```

`pod-eval.sh` runs one pack's whole bed on a rented GPU and writes a
baseline; `corpus-pod.sh` runs the bounded `kettle corpus` old/new
comparison on one (`evals/corpus/measurement-02-pod.md`) and never
writes a baseline or a tier. Both read `evals/RENTED-GPU.md`.

`reset-history.sh` belongs to a different job and is documented apart,
in `PUBLISHING.md`: how the three public repositories are produced from
this one — the measurement projection, the run archive and the demo at
kttl.app — why their history starts at the flip rather than at the first
commit, and the first-time setup behind the site.

`vendor-sidecar.sh` prepares a runtime and `publish-sidecar.sh`
validates it before replacing anything under `sidecars/`. The publisher
is an internal, network-free seam exercised by the ordinary runner
tests; call the vendor script for real work.

## The macOS packaging state, as measured

Kettle is macOS-first (#50). What follows was checked against a real
build on 27 July 2026, not inferred from documentation.

| Step | State |
|---|---|
| Bundle carries packs and a self-contained sidecar | done |
| Installer contains no model weights | done — 46MB, no `.gguf` |
| App and all 11 sidecar Mach-O files signed, hardened runtime | done |
| Notarised | **no** |
| Opens from a download without a warning | **no** — see below |

## Signing is local; notarisation is not

Two steps, often said in one breath, and only one of them sends anything
to Apple.

**Signing** runs entirely on your machine, against a certificate in your
keychain. `scripts/sign-macos.sh` does the sidecar; `tauri build` with
`APPLE_SIGNING_IDENTITY` set does the app. Nothing is transmitted. (A
*secure timestamp* is the one exception, and it sends a hash rather than
the binary — off by default here, `KETTLE_SECURE_TIMESTAMP=1` to enable.
Notarisation requires it.)

**Notarisation** uploads the built `.dmg` to Apple's automated scanner.
No human review, no App Store, no public listing, results in minutes —
but the binary does leave the machine. It is a deliberate, separate step
and nothing in this repo performs it.

## What Gatekeeper does with each

Measured with `spctl -a -t exec -vv`:

- **Unsigned or ad-hoc** — a downloaded copy is refused. The app is
  openable via right-click → Open, or by stripping the flag with
  `xattr -d com.apple.quarantine Kettle.app`.
- **Developer ID signed, not notarised** — still refused, with "Apple
  could not verify Kettle is free of malware". Signing alone does not
  fix distribution.
- **Signed, notarised and stapled** — opens with no warning.

Quarantine is what triggers all of this, and it is attached by whatever
delivers the file: browsers, AirDrop, Mail. A `.app` copied from
`target/release/bundle` or off a USB stick carries no quarantine flag,
which is why `smoke-install.sh` passes against a completely unsigned
bundle. **A green smoke test says nothing about Gatekeeper.**

## Two things that will bite

**The certificate is the wrong kind.** The identity on this machine is
*Apple Development*, which signs and verifies locally but Gatekeeper
will not accept and Apple will not notarise. Distribution outside the
App Store needs a *Developer ID Application* certificate — created once
from Xcode (Settings → Accounts → Manage Certificates → **+**) or the
developer portal, by the Account Holder. That is a certificate request;
nothing about the app is submitted. Current `spctl` verdict is
`rejected`, and it will stay rejected until both that certificate and
notarisation are in place.

**Tauri does not sign the sidecar, and `--deep` will tell you it did.**
`tauri build` signs the app bundle and leaves every Mach-O under
`Contents/Resources` ad-hoc and without the hardened runtime. Kettle
ships eleven of those. `codesign --verify --deep --strict` passes on
that bundle anyway, because files under `Resources/` are sealed as
resources by content hash rather than treated as nested code —
notarisation would reject it. Hence `sign-macos.sh`, run **before**
`tauri build`: the app's seal covers the resource bytes, so signing them
afterwards would invalidate it.

The check that actually catches this is per file:

```sh
find Kettle.app -type f -exec sh -c \
  'file -b "$1" | grep -q Mach-O && codesign -dv "$1" 2>&1 | grep -q adhoc && echo "$1"' _ {} \;
```

Anything it prints is a binary that would fail notarisation.

## A local prerequisite

`tauri build` calls `xattr -cr` when signing. If a Python `xattr`
package shadows `/usr/bin/xattr` on `PATH` — Homebrew installs one at
`/opt/homebrew/bin/xattr` — it does not support `-r` and the build fails
with `failed to remove extra attributes from app bundle`. Put `/usr/bin`
first:

```sh
cd app && PATH="/usr/bin:$PATH" APPLE_SIGNING_IDENTITY="<identity>" \
  bun run tauri build
```
