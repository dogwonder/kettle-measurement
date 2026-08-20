#!/usr/bin/env bash
#
# Vendor llama-server into sidecars/ (#50).
#
# The sidecar Kettle ships has to be the whole engine, not a launcher
# that finds its engine somewhere else. Homebrew's llama-server is 33KB
# and loads ten dylibs out of /opt/homebrew — bundle that and you have
# an installer that works on exactly one machine, the one that did not
# need it. So this takes pinned llama.cpp bytes, verifies their checksum,
# prepares the complete runtime, proves it starts, and only then publishes
# it into sidecars/.
#
# Pinned, not "latest", for the reason evals record a sidecar version at
# all: a llama-server upgrade can move a score while the weights are
# byte-identical (#74). Bumping the pin is a deliberate change with an
# eval run attached, not something that happens because somebody
# re-vendored on a Tuesday.
#
# Usage: scripts/vendor-sidecar.sh
# Then:  cargo test -p runner --test sidecar   (asserts what this made)

set -euo pipefail

# The pin. To move it: change the build and every checksum, run the
# script on each platform, run `cli eval <pack> --model <weights>
# --baseline evals/baseline.json`, and say what moved.
BUILD="b10145"

# The lab pin (#539, decided 20 August 2026).
#
# A measurement lab that cannot load anything newer than the product's
# build cannot find a ceiling, because the ceiling is always the newest
# thing: b10145 refuses Muse Glimmer outright and predates Qwen3.8, so
# four of five candidates on the 2026 list were blocked on the pin
# rather than on their merits.
#
# The answer is a second pin, not a moved one. #74 couples the product's
# pin to its scores deliberately — a llama-server upgrade moves numbers
# on byte-identical weights — so moving it would retire every committed
# baseline and every assurance claim resting on one, in exchange for
# being able to audition a candidate. The lab build lives beside the
# shipped one, at `sidecars/lab/<platform>`, and `kettle eval
# --sidecar-binary` is how a measurement opts into it.
#
# What that buys and what it does not: a lab-build number is a fact
# about that candidate on that runtime, and `SidecarInfo` records which
# runtime, so a recording still describes itself (#303). It is not
# comparable with a b10145 baseline, and nothing measured on it may fill
# tiers.json or back an assurance claim. Promoting a candidate means
# moving the *product* pin deliberately, with both packs re-recorded and
# what moved stated — which is the same bar as before, unchanged.
LAB_BUILD="b10516"

# Vendoring runs on the machine it vendors for, so it can use that
# platform's own tools to check its work. macOS consumes the upstream
# release; ARM64 Linux builds the same tag on the supported ABI floor.
case "$(uname -sm)" in
  "Darwin arm64")
    PLATFORM="macos-arm64"
    MODE="release"
    ARCHIVE="llama-$BUILD-bin-macos-arm64.tar.gz"
    URL="https://github.com/ggml-org/llama.cpp/releases/download/$BUILD/$ARCHIVE"
    SHA256="d1334c1a1d8fb38ffc82f239e201724534cbc712a4c7c12ff2da7563459fb6b7"
    ;;
  "Linux aarch64")
    PLATFORM="linux-arm64"
    MODE="source"
    ARCHIVE="llama.cpp-$BUILD.tar.gz"
    URL="https://github.com/ggml-org/llama.cpp/archive/refs/tags/$BUILD.tar.gz"
    SHA256="a75573ebc29b85b1743e4660663450aa3af9a8839bea97872775430840c8bf25"
    if [[ ! -r /etc/os-release ]]; then
      echo "cannot identify the ARM64 Linux ABI floor: /etc/os-release is absent" >&2
      exit 1
    fi
    # shellcheck disable=SC1091 -- this is the platform identity contract
    . /etc/os-release
    if [[ "${VERSION_CODENAME:-}" != "bookworm" ]]; then
      echo "linux-arm64 sidecars must be built on Debian/Pi OS bookworm" >&2
      echo "found ${PRETTY_NAME:-an unidentified Linux release}" >&2
      echo "building on a newer release would silently raise the glibc floor" >&2
      exit 1
    fi
    for tool in cmake cc c++; do
      if ! command -v "$tool" >/dev/null; then
        echo "building the bookworm sidecar requires $tool" >&2
        echo "install build-essential and cmake, then run this script again" >&2
        exit 1
      fi
    done
    ;;
  "Linux x86_64")
    # A **measurement** platform, not a shipping target. Kettle's
    # installer ships macOS today and the Pi build is the honesty check;
    # nothing here is published to a user. This case exists so a rented
    # GPU can run an eval at the *same pinned tag* the shipped sidecars
    # use — a llama-server upgrade can move a score while the weights
    # are byte-identical (#74), so a measurement taken on a different
    # build measures the build as much as the model.
    #
    # No ABI floor is asserted, and that is the difference: the floors
    # above exist because those binaries go in an installer and must
    # start on a stranger's machine. This one runs on a box that is
    # destroyed afterwards, so the machine it was built on is the only
    # machine it needs to start on. `GGML_NATIVE=ON` follows from the
    # same fact — tune for the host, since there is no other host.
    PLATFORM="linux-x86_64"
    MODE="source"
    ARCHIVE="llama.cpp-$BUILD.tar.gz"
    URL="https://github.com/ggml-org/llama.cpp/archive/refs/tags/$BUILD.tar.gz"
    # The same source tarball the ARM64 case takes, so the same digest.
    SHA256="a75573ebc29b85b1743e4660663450aa3af9a8839bea97872775430840c8bf25"
    for tool in cmake cc c++; do
      if ! command -v "$tool" >/dev/null; then
        echo "building the x86_64 sidecar requires $tool" >&2
        echo "install build-essential and cmake, then run this script again" >&2
        exit 1
      fi
    done
    CUDA="OFF"
    if command -v nvcc >/dev/null; then
      CUDA="ON"
    else
      echo "note: no nvcc found, building a CPU-only x86_64 sidecar" >&2
      echo "an eval on this will be correct and extremely slow" >&2
    fi
    ;;
  *)
    echo "no pinned llama.cpp build for $(uname -sm)" >&2
    echo "add one above with an explicit ABI floor and pinned checksum" >&2
    exit 1
    ;;
esac

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dest="$repo/sidecars/$PLATFORM"

# `--lab` vendors the lab pin alongside the shipped one instead of
# replacing it. Deliberately a separate destination: a lab build that
# overwrote `sidecars/<platform>` would silently re-runtime the product,
# and the next eval would compare against a baseline recorded on another
# llama-server without anything saying so.
if [[ "${1:-}" == "--lab" ]]; then
  BUILD="$LAB_BUILD"
  dest="$repo/sidecars/lab/$PLATFORM"
  case "$PLATFORM" in
    "macos-arm64")
      ARCHIVE="llama-$BUILD-bin-macos-arm64.tar.gz"
      URL="https://github.com/ggml-org/llama.cpp/releases/download/$BUILD/$ARCHIVE"
      SHA256="ee3324327d621026ae80c24031670e65fa62a0b23a3a027dbe2f65f240affd30"
      ;;
    *)
      echo "no lab pin for $PLATFORM yet: add its archive and checksum above" >&2
      echo "(the lab pin is vendored per platform, exactly as the shipped one is)" >&2
      exit 1
      ;;
  esac
  echo "→ lab pin $BUILD → sidecars/lab/$PLATFORM (the shipped pin stays at b10145)"
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "→ downloading $ARCHIVE"
curl -fsSL -o "$work/$ARCHIVE" "$URL"

echo "→ verifying checksum"
if command -v shasum >/dev/null; then
  got="$(shasum -a 256 "$work/$ARCHIVE" | cut -d' ' -f1)"
elif command -v sha256sum >/dev/null; then
  got="$(sha256sum "$work/$ARCHIVE" | cut -d' ' -f1)"
else
  echo "verifying the pinned source requires shasum or sha256sum" >&2
  exit 1
fi
if [[ "$got" != "$SHA256" ]]; then
  echo "checksum mismatch for $ARCHIVE" >&2
  echo "  expected $SHA256" >&2
  echo "  got      $got" >&2
  echo "pinned source that changed under its own tag is not something to work around" >&2
  exit 1
fi

tar xzf "$work/$ARCHIVE" -C "$work"
if [[ "$MODE" == "source" ]]; then
  source_root="$work/llama.cpp-$BUILD"
  if [[ "$PLATFORM" == "linux-x86_64" ]]; then
    echo "→ building $BUILD with CUDA=$CUDA"
  else
    echo "→ building $BUILD on the bookworm ABI floor"
  fi
  # The ARM64 case builds an ARMv8-A baseline so one bundle runs on both
  # the Pi 4 and Pi 5. The x86_64 case adds CUDA and otherwise leaves the
  # CPU backend generic.
  #
  # `GGML_NATIVE=ON` was the first draft here and it does not configure:
  # upstream refuses it alongside `GGML_BACKEND_DL`, which the shared
  # call below sets for every platform. The reasoning behind it was
  # wrong regardless — "tune for the one host you have" is an argument
  # about CPU kernels, and on a CUDA build the GPU runs the model. There
  # is nothing to buy.
  if [[ "$PLATFORM" == "linux-x86_64" ]]; then
    arch_flags=(-DGGML_NATIVE=OFF "-DGGML_CUDA=${CUDA}")
    # Blackwell (RTX 50-series) is compute capability 12.0, and a
    # default arch list that predates it produces a binary that builds
    # cleanly and then dies at runtime with no kernel image for the
    # device. Overridable, because the next card will need its own.
    if [[ "$CUDA" == "ON" ]]; then
      arch_flags+=("-DCMAKE_CUDA_ARCHITECTURES=${KETTLE_CUDA_ARCH:-120}")
    fi
  else
    arch_flags=(-DGGML_NATIVE=OFF -DGGML_CPU_ARM_ARCH=armv8-a)
  fi
  cmake -S "$source_root" -B "$source_root/build" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_RPATH='$ORIGIN' \
    -DCMAKE_BUILD_WITH_INSTALL_RPATH=ON \
    -DGGML_BACKEND_DL=ON \
    "${arch_flags[@]}" \
    -DGGML_CPU_ALL_VARIANTS=OFF \
    -DLLAMA_BUILD_NUMBER="${BUILD#b}" \
    -DLLAMA_BUILD_COMMIT=ad256ded \
    -DLLAMA_BUILD_TESTS=OFF \
    -DLLAMA_BUILD_EXAMPLES=OFF \
    -DLLAMA_BUILD_APP=OFF \
    -DLLAMA_BUILD_UI=OFF \
    -DLLAMA_USE_PREBUILT_UI=OFF
  cmake --build "$source_root/build" --config Release --target llama-server \
    --parallel "$(nproc)"
  src="$source_root/build/bin"
  cp "$source_root/LICENSE" "$src/LICENSE"
else
  src="$(dirname "$(find "$work" -name llama-server -type f -print -quit)")"
fi

if [[ -z "$src" || ! -f "$src/llama-server" ]]; then
  echo "no llama-server prepared from $ARCHIVE" >&2
  exit 1
fi

"$repo/scripts/publish-sidecar.sh" "$PLATFORM" "$src" "$dest" "$BUILD"
