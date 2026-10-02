#!/usr/bin/env bash
# Regenerates the pristine cluster and the share directory that main_main's
# `init` feature embeds (crates/backend/main/main_main/pristine/), from stock
# PostgreSQL 18.6, and prints the checksums the fork records.
#
# pgrust has no bootstrap mode (`--boot` is refused), so a data directory is
# made once, by stock initdb, and shipped: the approach pgrust's WASM demo
# takes with its packed datadir. Run this when the PostgreSQL version pgrust
# targets moves; commit the three files it writes together.
#
#   scripts/rustal-pristine.sh [PG_BINDIR]
#
# PG_BINDIR holds a PostgreSQL 18.6 `postgres`, `initdb` and `pg_config`
# (default: the bindir of the `pg_config` on PATH). Needs python3, for a
# reproducible archive: entries in name order, owner 0, mtime 0, a gzip header
# without a timestamp. The share archive's checksum is therefore the same on
# every run from the same PostgreSQL build; the cluster's is not, because
# initdb gives every cluster a new system identifier.
set -euo pipefail

want="18.6"
here=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
out="$here/crates/backend/main/main_main/pristine"
bindir=${1:-$(pg_config --bindir)}

die() { echo "rustal-pristine: $*" >&2; exit 1; }

for tool in postgres initdb pg_config; do
    [[ -x "$bindir/$tool" ]] || die "$bindir/$tool not found (pass a PostgreSQL $want bindir)"
done
version=$("$bindir/postgres" --version)
[[ "$version" =~ \(PostgreSQL\)\ ${want//./\\.}([^0-9]|$) ]] \
    || die "need PostgreSQL $want, found: $version"
share=$("$bindir/pg_config" --sharedir)
[[ -f "$share/postgres.bki" ]] || die "$share has no postgres.bki"
command -v python3 >/dev/null || die "python3 is required"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# A neutral cluster: C locale, UTF8, UTC (initdb otherwise writes the host's
# time zone into postgresql.conf), superuser `postgres`, trust auth. Whoever
# runs it decides who may connect (pg_hba.conf) and on what socket.
TZ=UTC LC_ALL=C "$bindir/initdb" -D "$work/data" --no-locale --encoding=UTF8 \
    -U postgres --auth=trust --no-instructions >"$work/initdb.log" 2>&1 \
    || { cat "$work/initdb.log" >&2; die "initdb failed"; }

mkdir -p "$out"
python3 - "$work/data" "$out/cluster.tar.gz" "$share" "$out/share.tar.gz" <<'PY'
import gzip, sys, tarfile

def reset(info):
    info.uid = info.gid = 0
    info.uname = info.gname = ""
    info.mtime = 0
    return info

def pack(src, dest):
    with open(dest, "wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0, compresslevel=9) as gz:
            # dereference: Homebrew's sharedir is a farm of symlinks into its
            # Cellar, and the archive must carry the files, not the links.
            with tarfile.open(fileobj=gz, mode="w", format=tarfile.PAX_FORMAT,
                              dereference=True) as tar:
                tar.add(src, arcname=".", filter=reset)  # recursion is name-ordered

pack(sys.argv[1], sys.argv[2])
pack(sys.argv[3], sys.argv[4])
PY

cd "$out"
{
    echo "# PostgreSQL: $version"
    shasum -a 256 cluster.tar.gz share.tar.gz
} >SHA256SUMS
cat SHA256SUMS
for f in cluster.tar.gz share.tar.gz; do echo "$(wc -c <"$f" | tr -d ' ') bytes  $f"; done
