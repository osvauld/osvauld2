"""Build the prototype debug APK, adding the Android inset listener's DEX to cargo-apk output.

Requires JAVA_HOME, Android SDK/API 35/build-tools 35.0.0 and the NDK used by cargo-apk.
    python3 scripts/build_android.py --target x86_64-linux-android
Signing uses the prototype debug key, not a production release key.
"""
import argparse
import json
import os
import shutil
import subprocess
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def run(*args):
    print(f"Android build: {args[0]}", flush=True)
    subprocess.run([str(a) for a in args], cwd=ROOT, check=True, timeout=600)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", default="aarch64-linux-android")
    args = parser.parse_args()
    sdk = Path(os.environ.get("ANDROID_HOME", Path.home() / "Android/Sdk"))
    tools = sdk / "build-tools/35.0.0"
    jar = sdk / "platforms/android-35/android.jar"
    javac = Path(os.environ["JAVA_HOME"]) / "bin/javac"
    run("cargo", "apk", "build", "-p", "android_app", "--lib", "--target", args.target)
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version=1"], cwd=ROOT))
    native_apk = Path(metadata["target_directory"]) / "debug/apk/osvauld-native.apk"
    apk = native_apk.with_name("osvauld.apk")
    with tempfile.TemporaryDirectory(prefix="osvauld-android-build-") as directory:
        temp = Path(directory)
        classes = temp / "classes"
        dex = temp / "dex"
        classes.mkdir()
        dex.mkdir()
        run(javac, "-J-Xmx256m", "--release", "8", "-cp", jar, "-d", classes,
            ROOT / "android_app/java/com/osvauld/android/InsetsAdapter.java")
        run(tools / "d8", "-JXmx256M", "--min-api", "28", "--lib", jar, "--output", dex,
            *classes.rglob("*.class"))
        unsigned = temp / "unsigned.apk"
        with zipfile.ZipFile(native_apk) as source, zipfile.ZipFile(unsigned, "w") as output:
            for entry in source.infolist():
                if entry.filename == "classes.dex" or entry.filename.startswith("META-INF/"):
                    continue
                with source.open(entry) as src, output.open(entry, "w") as dst:
                    shutil.copyfileobj(src, dst)
            output.write(dex / "classes.dex", "classes.dex", compress_type=zipfile.ZIP_DEFLATED)
        aligned = temp / "aligned.apk"
        run(tools / "zipalign", "-P", "16", "-f", "4", unsigned, aligned)
        run(tools / "apksigner", "sign", "--ks", Path.home() / ".android/debug.keystore",
            "--ks-key-alias", "androiddebugkey", "--ks-pass", "pass:android",
            "--key-pass", "pass:android", aligned)
        run(tools / "apksigner", "verify", aligned)
        staged = apk.with_suffix(".apk.pending")
        shutil.copyfile(aligned, staged)
        staged.replace(apk)
    print(f"APK with inset adapter: {apk}")


if __name__ == "__main__":
    main()
