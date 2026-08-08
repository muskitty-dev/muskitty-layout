#!/bin/bash
# Clone path dependencies for independent CI.
#
# muskitty-layout uses path dependencies for local development within the
# MusKitty workspace. When this repo is cloned standalone (e.g. on CI), the
# path deps must be materialized at the expected relative locations.
#
# Dependency chain:
#   muskitty-layout → muskitty-dom (path)
#                   → muskitty-cascade (path) → muskitty-css (+ parser + tokenizer)
#                   │                          → muskitty-cssom (+ css + parser + tokenizer)
#                   │                          → muskitty-dom
#                   │                          → muskitty-selectors (+ css + parser + tokenizer + dom)
#                   → muskitty-css (path) → muskitty-css-parser (path) → muskitty-css-tokenizer (path)
#                   → muskitty-cssom (path) → muskitty-css (+ parser + tokenizer)
#                   → muskitty-selectors (path) → muskitty-css (+ parser + tokenizer)
#                                              → muskitty-dom (path)
#   [dev-dep] muskitty-html5-parser (path) → muskitty-dom
#                                         → muskitty-html5-tokenizer (path)
#
# Almost all path deps are independent repos under muskitty-dev/. Exceptions:
# muskitty-cascade and muskitty-cssom are not yet stripped to standalone repos;
# they are pulled from the main workspace repo (Ink-dark/MusKitty) below.
#
# Auth: GitHub sometimes rate-limits anonymous git clones from CI runners,
# returning 401 and prompting for a username (which fails in non-interactive
# shells). When GH_TOKEN is provided via env, rewrite https://github.com/ URLs
# to use x-access-token auth. This works for any public repo the token can read.
#
# Idempotent: skips clones that already exist (useful for local re-runs).
set -euo pipefail

if [ -n "${GH_TOKEN:-}" ]; then
    git config --global "url.https://x-access-token:${GH_TOKEN}@github.com/".insteadOf "https://github.com/"
fi

clone_if_absent() {
    local url="$1"
    local dest="$2"
    if [ -d "$dest" ]; then
        echo "skip $dest (exists)"
    else
        git clone --depth 1 "$url" "$dest"
    fi
}

clone_if_absent https://github.com/muskitty-dev/muskitty-dom.git ../muskitty-dom
clone_if_absent https://github.com/muskitty-dev/muskitty-css.git ../muskitty-css
clone_if_absent https://github.com/muskitty-dev/muskitty-css-parser.git ../muskitty-css-parser
clone_if_absent https://github.com/muskitty-dev/muskitty-css-tokenizer.git ../muskitty-css-tokenizer
clone_if_absent https://github.com/muskitty-dev/muskitty-selectors.git ../muskitty-selectors
clone_if_absent https://github.com/muskitty-dev/muskitty-html5-parser.git ../muskitty-html5-parser
clone_if_absent https://github.com/muskitty-dev/muskitty-html5-tokenizer.git ../muskitty-html5-tokenizer

# ── Un-stripped crates (still in-tree members of the main workspace repo) ──
# cascade and cssom are not yet independent repos under muskitty-dev/. Pull
# their source from a shallow clone of Ink-dark/MusKitty instead. Their own
# path deps (muskitty-css/cssom/dom/selectors) resolve from this same ../ dir.
fetch_from_main_repo() {
    local crate="$1"
    if [ -d "../$crate" ]; then
        echo "skip ../$crate (exists)"
        return
    fi
    local tmp
    tmp=$(mktemp -d)
    git clone --depth 1 https://github.com/Ink-dark/MusKitty.git "$tmp"
    cp -r "$tmp/crates/$crate" "../$crate"
    rm -rf "$tmp"
}

fetch_from_main_repo muskitty-cascade
fetch_from_main_repo muskitty-cssom
