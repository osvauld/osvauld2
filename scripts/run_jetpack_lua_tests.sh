#!/bin/sh
# Every stock-Lua Jetpack test, from the worktree root. Needs `lua` (5.4) on PATH, e.g.
#   nix shell nixpkgs#lua --command sh scripts/run_jetpack_lua_tests.sh
set -e
for t in scripts/test_jetpack_*.lua; do
	echo "== $t"
	lua "$t"
done
