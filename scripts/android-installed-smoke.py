"""Install the release APK, verify native startup and exercise custom-scheme navigation."""
import pathlib
import subprocess
import time
import xml.etree.ElementTree as ET

PACKAGE = "com.a1var0w.safeeat"
OUT = pathlib.Path("artifacts/smoke")
OUT.mkdir(parents=True, exist_ok=True)


def adb(*args):
    return subprocess.check_output(["adb", *args], text=True)


def snapshot(name):
    adb("shell", "uiautomator", "dump", "/sdcard/safeeat.xml")
    adb("pull", "/sdcard/safeeat.xml", str(OUT / f"{name}.xml"))
    with (OUT / f"{name}.png").open("wb") as image:
        subprocess.run(["adb", "exec-out", "screencap", "-p"], stdout=image, check=True)
    tree = ET.parse(OUT / f"{name}.xml")
    return " ".join(node.get("text", "") + " " + node.get("content-desc", "") for node in tree.iter())


def wait_for(name, expected):
    for attempt in range(8):
        text = snapshot(name)
        if expected in text:
            return
        time.sleep(2)
    raise AssertionError(f"{name}: missing visible text: {expected}")


try:
    apk = next(pathlib.Path("artifacts").glob("SafeEat-*.apk"))
    adb("install", "-r", str(apk))
    adb("logcat", "-c")
    adb("shell", "am", "start", "-W", "-a", "android.intent.action.VIEW", "-d", "safeeat://home", PACKAGE)
    wait_for("home", "Buscar producto")
    assert adb("shell", "pidof", PACKAGE).strip(), "App process is absent"
    adb("shell", "am", "start", "-W", "-a", "android.intent.action.VIEW", "-d", "safeeat://auth/login", PACKAGE)
    wait_for("login", "Iniciar sesión")
    adb("shell", "am", "start", "-W", "-a", "android.intent.action.VIEW", "-d", "safeeat://p/" + "f" * 48, PACKAGE)
    wait_for("missing-share", "No hemos encontrado")
    (OUT / "result.txt").write_text("PASS: release APK installed; home, login and public-link error rendered; custom scheme resolved.\n")
finally:
    # No account credentials are entered by this smoke test.
    (OUT / "runtime.log").write_text(adb("logcat", "-d", "-s", "AndroidRuntime:E", "ReactNativeJS:E"))
