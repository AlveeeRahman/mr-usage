"""End-to-end check of the real app on Linux, driven through tauri-driver.

Usage: python3 -I e2e/drive.py <output-dir> <path-to-built-app>
Needs: tauri-driver listening on 127.0.0.1:4444 (with WebKitWebDriver), a
display, a running unlocked Secret Service, and `secret-tool`. Standard
library only. The key below is random and is not a real SerpApi key; the
vendor either rejects it (401) or is unreachable, and both paths must end in
an on-card error that does not contain the key.
"""
import base64, json, os, subprocess, sys, time, urllib.request

OUT = sys.argv[1]
APP = sys.argv[2]
KEY = "e2e0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0"
BASE = "http://127.0.0.1:4444"
results = []

def req(method, path, body=None):
    data = json.dumps(body).encode() if body is not None else None
    r = urllib.request.Request(BASE + path, data=data, method=method, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(r, timeout=60) as resp:
        return json.loads(resp.read() or b"{}").get("value")

def check(name, ok, detail=""):
    results.append((name, bool(ok), detail))
    print(("PASS " if ok else "FAIL ") + name + (f"  [{detail}]" if detail else ""), flush=True)

def secret_search():
    p = subprocess.run(["secret-tool", "search", "--all", "service", "mr-usage"], capture_output=True, text=True)
    return p.stdout + p.stderr

def new_session(attempts=3):
    """Launch the app under WebDriver. A cold machine can take longer than the
    driver's own startup wait, so one or two retries are normal."""
    for i in range(attempts):
        try:
            return req("POST", "/session", {"capabilities": {"alwaysMatch": {"tauri:options": {"application": APP}}}})["sessionId"]
        except Exception as e:
            print(f"session attempt {i + 1} failed: {e}", flush=True)
            if i == attempts - 1: raise
            time.sleep(5)

sid = new_session()
js = lambda script, *args: req("POST", f"/session/{sid}/execute/sync", {"script": script, "args": list(args)})
def shot(name):
    png = req("GET", f"/session/{sid}/screenshot")
    open(os.path.join(OUT, name), "wb").write(base64.b64decode(png))
def el(css):
    v = req("POST", f"/session/{sid}/element", {"using": "css selector", "value": css})
    return list(v.values())[0]
def wait_for(script, timeout=30):
    end = time.time() + timeout
    while time.time() < end:
        v = js(script)
        if v: return v
        time.sleep(0.5)
    return None

try:
    n = wait_for("return document.querySelectorAll('#add-provider option').length", 30)
    check("IPC works under connect-src 'none' (providers listed)", n >= 9, f"{n} providers")
    footer = js("return document.getElementById('footer').textContent")
    check("footer names the keystore", "Secret Service" in (footer or ""), footer)
    check("no inline style attributes in the DOM", js("return document.querySelectorAll('[style]').length") == 0)
    check("window.__TAURI__ global is absent", js("return typeof window.__TAURI__") == "undefined")
    shot("01-empty.png")

    js("const s=document.getElementById('add-provider'); s.value='serpapi'; s.dispatchEvent(new Event('change'));")
    check("endpoint shown before saving", js("return document.getElementById('add-endpoint').textContent") == "https://serpapi.com/account.json")
    req("POST", f"/session/{sid}/element/{el('#add-key')}/value", {"text": KEY})
    req("POST", f"/session/{sid}/element/{el('#add-save')}/click", {})
    status = wait_for("const t=document.getElementById('add-status').textContent; return t.startsWith('Saved')?t:null", 20)
    check("save_key succeeded", status, status)
    check("input cleared after save", js("return document.getElementById('add-key').value") == "")
    found = secret_search()
    check("key is in the OS keystore (Secret Service)", "serpapi" in found and "mr-usage" in found)
    hint = wait_for("const d=[...document.querySelectorAll('.facts dt')].find(x=>x.textContent==='Key'); return d?d.nextElementSibling.textContent:null", 10)
    check("masked key hint shown (last four characters only)", hint == "••••" + KEY[-4:], hint)
    check("full key never in the DOM", KEY not in js("return document.documentElement.outerHTML"))
    err = wait_for("const e=document.querySelector('.card .error'); return e?e.textContent:null", 40)
    check("first poll ran and the card shows an error for the fake key", err, err)
    check("error text does not contain the key", err is not None and KEY not in err)
    shot("02-key-saved-light.png")

    js("const t=document.getElementById('theme'); t.value='dark'; t.dispatchEvent(new Event('change'));")
    check("dark theme applied", wait_for("return document.documentElement.getAttribute('data-theme')==='dark'", 10))
    shot("03-key-saved-dark.png")

    r = js("return [...document.querySelectorAll('.card.keyed button')].map(b=>b.textContent)")
    check("keyed card has Refresh and Forget", "Refresh" in r and "Forget key" in r, str(r))
    check("only the keyed card is expanded", js("return document.querySelectorAll('.card.keyed').length") == 1)
    js("[...document.querySelectorAll('.card.keyed button')].find(b=>b.textContent==='Forget key').click()")
    time.sleep(0.3)
    js("[...document.querySelectorAll('.card.keyed button')].find(b=>b.textContent.startsWith('Click again')).click()")
    gone = wait_for("return document.querySelectorAll('.card.keyed').length===0 ? 'gone' : null", 10)
    check("forget removes the card", gone)
    check("forget removes the key from the OS keystore", "serpapi" not in secret_search())
    js("const t=document.getElementById('theme'); t.value='system'; t.dispatchEvent(new Event('change'));")
    shot("04-after-forget.png")
    before = js("return location.href")
    js("setTimeout(() => { location.href = 'https://example.com/'; }, 0); return true")
    time.sleep(3)
    after = js("return location.href")
    check("navigation guard blocks remote pages", after == before and "example.com" not in after, f"{before} -> {after}")
finally:
    try: req("DELETE", f"/session/{sid}")
    except Exception as e: print("delete session:", e)

json.dump(results, open(os.path.join(OUT, "results.json"), "w"), indent=1)
print("SUMMARY", sum(ok for _, ok, _ in results), "/", len(results))
