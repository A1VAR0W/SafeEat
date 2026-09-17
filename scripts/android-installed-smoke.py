"""Install the release APK, verify native startup and exercise custom-scheme navigation."""
import pathlib
import subprocess
import time
import xml.etree.ElementTree as ET
import json
import re
import secrets
import urllib.request
import uuid

PACKAGE = "com.a1var0w.safeeat"
OUT = pathlib.Path("artifacts/smoke")
OUT.mkdir(parents=True, exist_ok=True)


def adb(*args):
    result = subprocess.run(["adb", *args], text=True, capture_output=True)
    if result.returncode:
        raise RuntimeError("adb operation failed (arguments omitted to protect test credentials)")
    return result.stdout


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


def go(path):
    adb("shell", "am", "start", "-W", "-a", "android.intent.action.VIEW", "-d", f"safeeat://{path}", PACKAGE)


def tap(label, field=False):
    for attempt in range(9):
        snapshot("current")
        nodes = ET.parse(OUT / "current.xml").iter()
        for node in nodes:
            content = node.get("content-desc", "")
            text = node.get("text", "")
            matches = content == label or text == label or (not field and label in content)
            kind = "EditText" in node.get("class", "") if field else node.get("clickable") == "true"
            if matches and kind and node.get("enabled") == "true":
                x1, y1, x2, y2 = map(int, re.findall(r"\d+", node.get("bounds")))
                adb("shell", "input", "tap", str((x1+x2)//2), str((y1+y2)//2))
                return
        # Scroll within the app, never through a system dialog.
        adb("shell", "input", "swipe", "540", "1500", "540", "600", "250")
        time.sleep(1)
    raise AssertionError(f"Control unavailable: {label}")


def fill(label, value):
    tap(label, field=True)
    # Values are generated alphanumeric credentials; never printed or stored in artifacts.
    # A single input-text burst can outrun controlled React Native TextInput updates.
    # Send settled keystrokes as a person would, then verify the complete non-secret value.
    for character in value:
        adb("shell", "input", "text", character)
        time.sleep(0.05)
    adb("shell", "input", "keyevent", "4")
    snapshot("filled")
    node = next(n for n in ET.parse(OUT / "filled.xml").iter()
                if "EditText" in n.get("class", "") and n.get("content-desc") == label)
    if node.get("password") != "true":
        assert node.get("text") == value, f"Input automation did not fill {label} completely"


environment = json.loads(pathlib.Path("artifacts/SafeEat-environment.json").read_text())
origin = environment["apiUrl"]
credentials = {"email": f"apk-{uuid.uuid4().hex[:12]}@example.org", "password": secrets.token_hex(16)}
server_session = None
checks = []


def http(path, method="GET", payload=None, token=None):
    data = json.dumps(payload).encode() if payload is not None else None
    headers = {"Content-Type": "application/json"} if data else {}
    if token:
        headers["Authorization"] = "Bearer " + token
    request = urllib.request.Request(origin + "/api/v1" + path, data=data, headers=headers, method=method)
    with urllib.request.urlopen(request, timeout=20) as response:
        raw = response.read()
        return json.loads(raw) if raw else None


def passed(name):
    checks.append({"name": name, "status": "PASS"})
    print("PASS:", name, flush=True)


try:
    apk = next(pathlib.Path("artifacts").glob("SafeEat-*.apk"))
    adb("install", "-r", str(apk))
    adb("logcat", "-c")
    assert not adb("reverse", "--list").strip(), "adb reverse must not be configured"
    go("home")
    wait_for("home", "Buscar producto")
    assert adb("shell", "pidof", PACKAGE).strip(), "App process is absent"
    passed("Standalone Release startup without Metro or adb reverse")
    go("settings")
    tap("Comprobar conexión con SafeEat")
    wait_for("connection", "Conexión correcta")
    assert environment["environment"] in snapshot("connection")
    passed("APK contacts the intended environment over public HTTPS")
    go("auth/register")
    fill("Correo electrónico", credentials["email"])
    fill("Contraseña", credentials["password"])
    tap("Crear cuenta")
    wait_for("registered", "Buscar producto")
    passed("Registration through the installed APK")
    go("profile")
    wait_for("profile", credentials["email"])
    tap("Cerrar sesión")
    wait_for("logged-out", "Iniciar sesión")
    go("auth/login")
    fill("Correo electrónico", credentials["email"])
    fill("Contraseña", credentials["password"])
    tap("Iniciar sesión")
    wait_for("logged-in", "Buscar producto")
    adb("shell", "am", "force-stop", PACKAGE)
    go("profile")
    wait_for("restored", credentials["email"])
    passed("Login, logout and SecureStore session restoration after process restart")
    go("home")
    fill("Buscar producto", "arroz")
    tap("Buscar")
    wait_for("search", "Resultados")
    # Pick an actual returned product, using its accessible product-row label.
    label = None
    for attempt in range(8):
        snapshot("search")
        for node in ET.parse(OUT / "search.xml").iter():
            desc = node.get("content-desc", "")
            if node.get("clickable") == "true" and any(desc.endswith(status) for status in ["NO CONCLUYENTE", "SIN GLUTEN DECLARADO", "CONTIENE GLUTEN", "PUEDE CONTENER TRAZAS"]):
                label = desc
                break
        if label:
            break
        adb("shell", "input", "swipe", "540", "1500", "540", "650", "250")
        time.sleep(2)
    assert label, "Search returned no accessible product rows"
    product_name = label.rsplit(". ", 1)[0]
    tap(label)
    tap("Guardar en favoritos")
    wait_for("saved", "Quitar de favoritos")
    passed("Real OFF search, canonical product detail and favorite mutation")
    go("favorites")
    wait_for("favorites", product_name)
    # Verify the APK's mutation independently through the public API, not just optimistic UI.
    server_session = http("/auth/login", "POST", credentials)
    saved = http("/favorites/products", token=server_session["accessToken"])
    assert any(item["name"] == product_name for item in saved["items"])
    passed("Favorite persisted in the production database and reloaded by APK")
    tap(product_name)
    tap("Compartir")
    share_url = None
    for attempt in range(5):
        text = snapshot("native-share")
        match = re.search(re.escape(environment["publicUrl"]) + r"/p/[a-f0-9]{48}", text)
        if match:
            share_url = match.group(0)
            break
        time.sleep(2)
    assert share_url, "Native sharesheet did not expose the environment-correct share URL"
    with urllib.request.urlopen(share_url, timeout=20) as public:
        assert public.status == 200
        assert 'property="og:title"' in public.read().decode()
    passed("Native sharing and anonymous public Open Graph viewer")
    adb("shell", "input", "keyevent", "4")
    go("favorites")
    tap(product_name)
    tap("Quitar de favoritos")
    go("favorites")
    wait_for("removed", "Todavía no has guardado")
    assert not http("/favorites/products", token=server_session["accessToken"])["items"]
    passed("Unfavorite persisted after bodyless DELETE")
    go("profile")
    tap("Cerrar sesión")
    wait_for("final-logout", "Iniciar sesión")
    go("p/" + "f" * 48)
    wait_for("missing-share", "No hemos encontrado")
    passed("Logout and custom-scheme 404 over the Internet")
finally:
    try:
        if server_session is None:
            server_session = http("/auth/login", "POST", credentials)
        http("/auth/me", "DELETE", token=server_session["accessToken"])
        print("Disposable APK account removed", flush=True)
    except Exception:
        print("Cleanup needs verification if APK registration succeeded", flush=True)
    (OUT / "result.json").write_text(json.dumps({"environment": environment, "checks": checks}, indent=2))
    (OUT / "runtime.log").write_text(adb("logcat", "-d", "-s", "AndroidRuntime:E", "ReactNativeJS:E"))
