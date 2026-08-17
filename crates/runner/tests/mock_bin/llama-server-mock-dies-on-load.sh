#!/bin/sh
# Mock llama-server that dies on the way up, the way a mislinked binary
# does: the dynamic loader refuses it, so it writes to stderr and exits
# before it ever binds a port. This is the Raspberry Pi 5 failure — an
# ubuntu-arm64 build against glibc 2.38 on a Pi OS bookworm with 2.36 —
# and it must not read as a slow model load.
echo "llama-server: /lib/aarch64-linux-gnu/libc.so.6: version \`GLIBC_2.38' not found" >&2
exit 127
