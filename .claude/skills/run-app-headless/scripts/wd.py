"""A dependency-free WebDriver client for driving the scryer app through
tauri-driver. Usage: python3 wd.py <start|open|js|shot|click|resize|stop> [args]."""
import base64, json, os, sys, time, urllib.request

BASE = "http://127.0.0.1:4444"
RUN_DIR = os.environ.get("RUN_DIR", "/tmp/scryer-headless")
SID = os.path.join(RUN_DIR, "session")
ADDED = os.path.join(RUN_DIR, "added-projects.json")


def call(method, path, body=None):
    req = urllib.request.Request(
        BASE + path, method=method,
        data=None if body is None else json.dumps(body).encode(),
        headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=180) as r:
        return json.loads(r.read() or b"{}").get("value")


def sid():
    return open(SID).read().strip()


def js(script, *args):
    return call("POST", f"/session/{sid()}/execute/sync", {"script": script, "args": list(args)})


def wait_for(script, timeout=60):
    end = time.time() + timeout
    while time.time() < end:
        if js(script):
            return True
        time.sleep(0.5)
    return False


def main(cmd, *args):
    if cmd == "start":
        v = call("POST", "/session", {"capabilities": {"alwaysMatch": {"tauri:options": {"application": args[0]}}}})
        open(SID, "w").write(v["sessionId"])
        wait_for("return document.readyState === 'complete'")
        print(v["sessionId"])
    elif cmd == "open":
        path = os.path.abspath(args[0])
        added = js("""
            const p = arguments[0];
            const r = JSON.parse(localStorage.getItem('scryer:recent-projects') || '[]');
            const had = r.includes(p);
            localStorage.setItem('scryer:recent-projects', JSON.stringify([p, ...r.filter(x => x !== p)]));
            location.reload();
            return !had;""", path)
        if added:
            json.dump([path], open(ADDED, "w"))
        wait_for("return [...document.querySelectorAll('button')].some(b => b.innerText.includes(arguments[0]))".replace("arguments[0]", json.dumps(os.path.basename(path))))
        js("[...document.querySelectorAll('button')].find(b => b.innerText.includes(arguments[0])).click()", os.path.basename(path))
        ok = wait_for("return !!document.querySelector('[data-tauri-drag-region]') && document.body.innerText.includes('MODEL')", 120)
        print("opened" if ok else "workspace did not appear")
    elif cmd == "js":
        print(json.dumps(js(args[0]), indent=1)[:8000])
    elif cmd == "shot":
        png = call("GET", f"/session/{sid()}/screenshot")
        open(args[0], "wb").write(base64.b64decode(png))
        print("saved", args[0])
    elif cmd == "click":
        print(js("""
            const t = arguments[0];
            const el = [...document.querySelectorAll('button,a,li,[role=menuitem]')].find(e => e.innerText.trim() === t);
            if (!el) return 'not found';
            el.click(); return 'clicked';""", args[0]))
    elif cmd == "resize":
        call("POST", f"/session/{sid()}/window/rect", {"width": int(args[0]), "height": int(args[1])})
    elif cmd == "stop":
        if os.path.exists(ADDED):
            js("""
                const gone = new Set(arguments[0]);
                const r = JSON.parse(localStorage.getItem('scryer:recent-projects') || '[]');
                localStorage.setItem('scryer:recent-projects', JSON.stringify(r.filter(p => !gone.has(p))));""",
               json.load(open(ADDED)))
            os.remove(ADDED)
        call("DELETE", f"/session/{sid()}")
        os.remove(SID)
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main(*sys.argv[1:])
