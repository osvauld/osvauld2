"""osvauld bridge client — Python automation for the osvauld2 shell.

Speaks `osvauld-rpc` over the shell's UDS socket: 4-byte big-endian length
prefix + JSON payload. Mirrors the old repo's `scripts/osvauld` harness,
trimmed to this vocabulary.
"""
