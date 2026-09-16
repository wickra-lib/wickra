#!/usr/bin/env bash
#
# Regenerate every committed lockfile in the workspace:
#   - Rust:   Cargo.lock, fuzz/Cargo.lock        (cargo update)
#   - Node:   bindings/node/package-lock.json     (npm install --package-lock-only)
#   - Python: .github/requirements/*.txt          (uv pip compile --generate-hashes)
#
# Run from anywhere; the script cd's to the repo root itself:
#
#     ./scripts/update-lockfiles.sh
#
# The Python locks are hash-pinned (OpenSSF Scorecard PinnedDependencies) and
# generated with uv rather than pip-tools because uv can resolve a *target*
# Python version's full transitive closure — with hashes — without that
# interpreter being installed locally. That is required for the numpy cp39/cp313
# split: numpy ships no single release with wheels for both, so ci-dev is locked
# twice (Python 3.9 and Python 3.10+). If uv is not on PATH the script stops and
# tells you to install it (https://docs.astral.sh/uv/getting-started/installation/);
# WICKRA_BOOTSTRAP_UV=1 opts into fetching one pinned, checksum-verified release
# into a temporary directory instead.
#
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "==> Rust (Cargo.lock, fuzz/Cargo.lock)"
cargo update
(cd fuzz && cargo update)

echo "==> Node (bindings/node/package-lock.json)"
(cd bindings/node && npm install --package-lock-only --no-audit --no-fund)

echo "==> Python (.github/requirements/*.txt via uv)"
# uv is not installed for you unless you ask. The previous version piped
# https://astral.sh/uv/install.sh straight into a shell, which runs whatever is
# behind that URL at that moment, with your privileges, on the machine of
# everyone who regenerates a lockfile. Set WICKRA_BOOTSTRAP_UV=1 to opt in; the
# bootstrap then fetches one pinned release archive and refuses to use it unless
# its checksum matches the one recorded here.
UV_VERSION="0.12.15"
uv_sha256() {
  case "$1" in
    x86_64-unknown-linux-gnu)  echo "f97935763c04be3e692460a7aaeaaab8fc3b78fcf8b389da820b38ae7423a638" ;;
    aarch64-unknown-linux-gnu) echo "0e9a3499b0587d449c9ff684c0160da607826e4af1cee220bc87f378702d3e08" ;;
    aarch64-apple-darwin)      echo "dc304b9ed1b24174572290fba60ac3f6fe63c73a671f0439e62a91375841964d" ;;
    x86_64-apple-darwin)       echo "e9ca61775532368fe518ab03e7a354c7ecab8ccb3c7d941c775fcc4a362b801b" ;;
    *)                         echo "" ;;
  esac
}

if ! command -v uv >/dev/null 2>&1; then
  if [ "${WICKRA_BOOTSTRAP_UV:-0}" != "1" ]; then
    echo "    uv is not on PATH." >&2
    echo "    Install it (https://docs.astral.sh/uv/getting-started/installation/)," >&2
    echo "    or re-run with WICKRA_BOOTSTRAP_UV=1 to fetch uv ${UV_VERSION} here." >&2
    exit 1
  fi

  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)   uv_target="x86_64-unknown-linux-gnu" ;;
    Linux-aarch64)  uv_target="aarch64-unknown-linux-gnu" ;;
    Darwin-arm64)   uv_target="aarch64-apple-darwin" ;;
    Darwin-x86_64)  uv_target="x86_64-apple-darwin" ;;
    *)
      echo "    No pinned uv build for $(uname -s)-$(uname -m); install uv yourself." >&2
      exit 1
      ;;
  esac
  uv_expected="$(uv_sha256 "$uv_target")"

  echo "    bootstrapping uv ${UV_VERSION} (${uv_target})..."
  uv_dir="$(mktemp -d)"
  trap 'rm -rf "$uv_dir"' EXIT
  uv_archive="uv-${uv_target}.tar.gz"
  curl -fsSL --retry 5 --retry-all-errors -o "${uv_dir}/${uv_archive}"     "https://github.com/astral-sh/uv/releases/download/${UV_VERSION}/${uv_archive}"
  echo "${uv_expected}  ${uv_dir}/${uv_archive}" | sha256sum -c -
  tar -xzf "${uv_dir}/${uv_archive}" -C "$uv_dir" --strip-components=1
  export PATH="${uv_dir}:$PATH"
fi

req=".github/requirements"
cc="./scripts/update-lockfiles.sh"
uv pip compile --quiet --python-version 3.9  --generate-hashes --custom-compile-command "$cc" "$req/ci-dev-py39.in" -o "$req/ci-dev-py39.txt"
uv pip compile --quiet --python-version 3.11 --generate-hashes --custom-compile-command "$cc" "$req/ci-dev-py3.in"  -o "$req/ci-dev-py3.txt"
uv pip compile --quiet --python-version 3.11 --generate-hashes --custom-compile-command "$cc" "$req/bench.in"       -o "$req/bench.txt"

echo "==> Done. Review 'git diff' before committing."
