#!/bin/sh
# Every stock-Lua Brawl test, from the worktree root. Needs `lua` 5.4 on PATH (`lua5.4` is used if
# present), e.g. nix shell nixpkgs#lua5_4 --command sh scripts/run_brawl_lua_tests.sh
set -e
LUA=$(command -v lua5.4 || command -v lua)
for t in scripts/test_brawl_*.lua; do
	echo "== $t"
	"$LUA" "$t"
done
