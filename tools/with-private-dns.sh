#!/bin/bash
# Runs one build command with its own view of /etc/resolv.conf (private mount namespace, no privileges,
# no system change). For the days the system resolver is not usable; a normal machine does not need this.
set -eu
D=/mnt/marz-lab/toolchains/desktop/ns
exec unshare -Urm sh -c 'mount --bind "$0/resolv.conf" /etc/resolv.conf; mount --bind "$0/nsswitch.conf" /etc/nsswitch.conf; exec "$@"' "$D" "$@"
