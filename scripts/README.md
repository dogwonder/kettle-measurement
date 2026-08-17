# scripts/

Packaging, in the order it happens. Every one of these needs something
gitignored or a real build, so none of them run in CI — the same
position as the evals, and for the same reason.

```
vendor-sidecar.sh   →   sign-macos.sh   →   tauri build   →   smoke-install.sh
   the engine            the signature       the bundle        does it stand up
```

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
