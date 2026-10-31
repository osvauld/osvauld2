"""Supply graphics libraries and ffmpeg to a smoke command on the NixOS worker.

Run through tasks2 --peer nixos. Packages are realised in the Nix store, without changing the
worker's system profile. Use with_xvfb.py when there is no desktop session.
"""
import json
import os
from pathlib import Path
import subprocess
import sys


PACKAGES = r'''
let
  p = (builtins.getFlake (builtins.getEnv "OSVAULD_NIXPKGS")).legacyPackages.x86_64-linux;
  pick = lower: legacy:
    if builtins.hasAttr lower p then builtins.getAttr lower p
    else if builtins.hasAttr legacy p then builtins.getAttr legacy p
    else builtins.getAttr legacy p.xorg;
in [ (p.lib.getBin p.ffmpeg) ] ++ map p.lib.getLib [
  (pick "libx11" "libX11") (pick "libxcursor" "libXcursor")
  (pick "libxrandr" "libXrandr") (pick "libxi" "libXi")
  p.libxkbcommon p.vulkan-loader
  (if p.mesa ? drivers then p.mesa.drivers else p.mesa)
]
'''


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    env = dict(os.environ)
    if not env.get("OSVAULD_NIXPKGS"):
        version = json.loads(subprocess.check_output(["nixos-version", "--json"], text=True))
        # Match the executable's host ABI, rather than mixing a newer Mesa/LLVM with old glibc.
        env["OSVAULD_NIXPKGS"] = "github:NixOS/nixpkgs/" + version["nixpkgsRevision"]
    result = subprocess.check_output([
        "nix", "--extra-experimental-features", "nix-command flakes", "build",
        "--impure", "--no-link", "--json", "--expr", PACKAGES,
    ], text=True, env=env)
    paths = [Path(value) for entry in json.loads(result) for value in entry["outputs"].values()]
    libraries = [str(path / "lib") for path in paths if (path / "lib").is_dir()]
    libraries += [env[key] for key in ("LD_LIBRARY_PATH", "NIX_LD_LIBRARY_PATH") if env.get(key)]
    env["LD_LIBRARY_PATH"] = os.pathsep.join(libraries)
    env["PATH"] = os.pathsep.join([str(path / "bin") for path in paths
                                 if (path / "bin").is_dir()] + [env["PATH"]])
    software = [str(icd) for path in paths for icd in
                (path / "share/vulkan/icd.d").glob("*lvp*.json")]
    if software:
        env["VK_ICD_FILENAMES"] = os.pathsep.join(software)
    print("Nix worker graphics environment ready", flush=True)
    return subprocess.run(sys.argv[1:], env=env).returncode


if __name__ == "__main__":
    raise SystemExit(main())
