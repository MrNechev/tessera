#!/usr/bin/env python3
"""Contract test: compares the Letopis (Go) and Tessera (Rust) HTTP APIs on the same library.

    python3 -I compare.py --go http://127.0.0.1:28470 --rs http://127.0.0.1:28480 --password pw

Fields that legitimately differ (ids, timestamps, version string) are ignored. Prints every
difference and exits non-zero if there are any (apart from the allowed ones listed in
ALLOWED).
"""
import argparse
import io
import json
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile
from http.cookiejar import CookieJar

IGNORE_KEYS = {"id", "createdAt", "updatedAt", "savedAt", "docId", "sectionId", "token", "urls",
               "uptime", "exported"}
# Rust-only additions that are allowed
EXTRA_OK = {("libraries", "writable")}
# differences that are expected: JSON parser error texts differ between Go and serde (and
# the product name is renamed)
ALLOWED = [("POST /api/documents/import", ".error")]


class Client:
    def __init__(self, base, name):
        self.base = base.rstrip("/")
        self.name = name
        self.jar = CookieJar()
        self.op = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(self.jar))

    def req(self, method, path, body=None, headers=None, raw=False):
        data = None
        h = dict(headers or {})
        if body is not None:
            if isinstance(body, (bytes, str)):
                data = body.encode() if isinstance(body, str) else body
            else:
                data = json.dumps(body).encode()
                h.setdefault("Content-Type", "application/json")
        path = urllib.parse.quote(path, safe="/?&=%:,+@!$'()*;~-._")
        r = urllib.request.Request(self.base + path, data=data, method=method, headers=h)
        try:
            resp = self.op.open(r, timeout=180)
            status, hdrs, content = resp.status, resp.headers, resp.read()
        except urllib.error.HTTPError as e:
            status, hdrs, content = e.code, e.headers, e.read()
        if raw:
            return status, hdrs, content
        try:
            js = json.loads(content) if content else None
        except ValueError:
            js = content.decode(errors="replace")
        return status, js

    def get(self, path):
        return self.req("GET", path)


DIFFS = []


def norm(v, path=""):
    if isinstance(v, dict):
        return {k: norm(x, path + "." + k) for k, x in v.items() if k not in IGNORE_KEYS}
    if isinstance(v, list):
        return [norm(x, path) for x in v]
    if isinstance(v, float) and v.is_integer():
        return int(v)
    return v


def diff(a, b, where, path=""):
    if isinstance(a, dict) and isinstance(b, dict):
        for k in sorted(set(a) | set(b)):
            if k not in b:
                DIFFS.append((where, path + "." + k, a[k], "<missing>"))
            elif k not in a:
                if (where.split("?")[0].split("/")[-1], k) in EXTRA_OK or k == "writable":
                    continue
                DIFFS.append((where, path + "." + k, "<missing>", b[k]))
            else:
                diff(a[k], b[k], where, path + "." + k)
        return
    if isinstance(a, list) and isinstance(b, list):
        if len(a) != len(b):
            DIFFS.append((where, path + "[len]", len(a), len(b)))
        for i, (x, y) in enumerate(zip(a, b)):
            diff(x, y, where, f"{path}[{i}]")
        return
    if isinstance(a, (int, float)) and isinstance(b, (int, float)) and not isinstance(a, bool):
        if abs(a - b) <= 1e-6 * max(1, abs(a), abs(b)):
            return
    if a != b:
        DIFFS.append((where, path, a, b))


def compare(go, rs, method, path, body=None, transform=None, status_only=False):
    sg, jg = go.req(method, path, body)
    sr, jr = rs.req(method, path, body)
    where = f"{method} {path}"
    if sg != sr:
        DIFFS.append((where, "status", sg, sr))
    if status_only:
        return jg, jr
    a, b = norm(jg), norm(jr)
    if transform:
        a, b = transform(a), transform(b)
    diff(a, b, where)
    return jg, jr


def wait_ready(c, timeout=420):
    t0 = time.time()
    while time.time() - t0 < timeout:
        s, st = c.get("/api/status")
        if s == 200 and not st["scan"]["running"] and st["counts"]["total"] > 0 and st["counts"]["unprobed"] == 0:
            pools = st["pools"]
            busy = sum(p["running"] + p["queuedHigh"] + p["queuedLow"] for p in pools)
            _, d = c.get("/api/duplicates")
            if busy == 0 and d["hashPending"] <= 1:
                return st
        time.sleep(2)
    raise SystemExit(f"{c.name}: not ready after {timeout}s")


def by_path(items):
    return sorted(items, key=lambda f: (f.get("libraryId", ""), f.get("path", "")))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--go", required=True)
    ap.add_argument("--rs", required=True)
    ap.add_argument("--password", required=True)
    a = ap.parse_args()
    go, rs = Client(a.go, "go"), Client(a.rs, "rs")

    # ---------- session / login ----------
    for c in (go, rs):
        s, js = c.get("/api/session")
        assert s == 200 and js["authEnabled"] and not js["authenticated"], (c.name, js)
    compare(go, rs, "GET", "/api/libraries")  # 401 both
    compare(go, rs, "POST", "/api/login", {"password": "wrong"})
    for c in (go, rs):
        s, js = c.req("POST", "/api/login", {"password": a.password})
        assert s == 200 and js == {"ok": True}, (c.name, s, js)
    compare(go, rs, "GET", "/api/session", transform=lambda j: {k: v for k, v in j.items() if k != "version"})

    print("waiting for scans, probes and thumbnails…", flush=True)
    wait_ready(go)
    wait_ready(rs)

    # ---------- library ----------
    compare(go, rs, "GET", "/api/libraries")
    compare(go, rs, "GET", "/api/libraries/main/tree")
    compare(go, rs, "GET", "/api/libraries/nope/tree")
    queries = [
        "library=main&dir=&recursive=1",
        "library=main&dir=trips",
        "library=main&dir=trips&recursive=1&sort=name",
        "library=main&recursive=1&sort=size&desc=1",
        "library=main&recursive=1&sort=mtime&desc=1",
        "library=main&recursive=1&sort=taken",
        "library=main&recursive=1&sort=duration&desc=1",
        "library=main&recursive=1&kind=image,gif",
        "library=main&recursive=1&kind=video&minShort=1080",
        "q=ICELAND",
        "q=котка",
        "library=main&recursive=1&gps=1",
        "library=main&recursive=1&motion=1",
        "library=main&recursive=1&camera=" + urllib.parse.quote("Apple iPhone 15 Pro"),
        "library=main&recursive=1&orientation=portrait",
        "library=main&recursive=1&orientation=landscape",
        "library=main&recursive=1&from=1600000000000&to=1720000000000",
        "library=main&recursive=1&minSize=100000&maxSize=900000000",
        "library=main&recursive=1&minDur=2.5&maxDur=10",
        "library=main&recursive=1&limit=3&offset=2&sort=name",
        "kind=all&limit=5000",
    ]
    def items_by_path(j):
        if isinstance(j, dict) and "items" in j:
            j = dict(j, items=by_path(j["items"]))
        return j
    for q in queries:
        # ties in the sort key are broken by row id, which differs between servers
        tf = None if "limit=3" in q else items_by_path
        compare(go, rs, "GET", "/api/files?" + q, transform=tf)
    _, all_go = go.get("/api/files?library=main&recursive=1&limit=2000")
    _, all_rs = rs.get("/api/files?library=main&recursive=1&limit=2000")
    paths = [f["path"] for f in all_go["items"]]
    print(f"{len(paths)} files indexed (go) / {all_rs['total']} (rust)")
    rs_ids = {f["path"]: f["id"] for f in all_rs["items"]}
    for f in all_go["items"]:
        sg, jg = go.get(f"/api/files/{f['id']}")
        sr, jr = rs.get(f"/api/files/{rs_ids.get(f['path'], 0)}")
        diff(norm(jg), norm(jr), f"GET /api/files/{{id}} ({f['path']})")
    compare(go, rs, "GET", "/api/files/999999")

    refs = [{"libraryId": "main", "path": p} for p in paths]
    refs += [{"libraryId": "main", "path": "../etc/passwd"}, {"libraryId": "nope", "path": "a.jpg"},
             {"libraryId": "main", "path": "missing.jpg"}, {"libraryId": "main", "path": "./trips//portrait.jpg"}]
    compare(go, rs, "POST", "/api/resolve", {"refs": refs})
    compare(go, rs, "POST", "/api/resolve", "{not json")
    def match_norm(j):
        return [dict(r, candidates=by_path(r["candidates"])) for r in j] if isinstance(j, list) else j
    compare(go, rs, "POST", "/api/match", transform=match_norm, body={"items": [
        {"name": "reykjavik.jpg", "size": 0},
        {"name": "REYKJAVIK.JPG", "size": next(f["size"] for f in all_go["items"] if f["path"] == "trips/iceland/reykjavik.jpg")},
        {"path": "/media/main/trips/iceland/reykjavik.jpg"},
        {"path": "main:trips/portrait.jpg"},
        {"path": "/home/someone/Pictures/trips/fractal.png"},
        {"path": "C:\\Users\\x\\trips\\iceland\\drive.mp4"},
        {"path": "/media/main/%D0%A1%D0%BC%D0%B5%D1%88%D0%BD%D0%B8/%D0%BA%D0%BE%D1%82%D0%BA%D0%B0.gif"},
        {"name": "nothing.jpg"},
    ]})
    compare(go, rs, "GET", "/api/cameras")

    def dup_norm(j):
        groups = sorted(
            (g["kind"], tuple(sorted(f["path"] for f in g["files"]))) for g in j["groups"])
        return {"groups": groups, "hashPending": j["hashPending"]}
    compare(go, rs, "GET", "/api/duplicates", transform=dup_norm)
    compare(go, rs, "GET", "/api/duplicates?dist=12", transform=dup_norm)

    # ---------- prefs ----------
    compare(go, rs, "GET", "/api/prefs")
    compare(go, rs, "PUT", "/api/prefs", {"favorites": [{"libraryId": "main", "path": "trips/", "dir": True},
                                                         {"libraryId": "main", "path": "../x"},
                                                         {"libraryId": "", "path": "a"}],
                                           "addRecent": [{"libraryId": "main", "path": "trips/portrait.jpg"}]},
            transform=lambda j: {k: [{kk: vv for kk, vv in i.items() if kk != "at"} for i in v] for k, v in j.items()})
    compare(go, rs, "PUT", "/api/prefs", {"recent": []})

    # ---------- documents ----------
    compare(go, rs, "GET", "/api/documents")
    dg, dr = compare(go, rs, "POST", "/api/documents", {"title": "  Trip to Iceland  "})
    ids = (dg["id"], dr["id"])

    def both(method, fmt, body=None, **kw):
        sg, jg = go.req(method, fmt.format(ids[0]), body)
        sr, jr = rs.req(method, fmt.format(ids[1]), body)
        where = f"{method} {fmt}"
        if sg != sr:
            DIFFS.append((where, "status", sg, sr))
        if not kw.get("status_only"):
            a, b = norm(jg), norm(jr)
            if kw.get("tf"):
                a, b = kw["tf"](a), kw["tf"](b)
            diff(a, b, where)
        return jg, jr

    doc = {"title": "Trip to Iceland", "tags": [" iceland ", "Iceland", "roadtrip"],
           "settings": {"transition": "fade", "music": {"libraryId": "main", "path": "music/tone.mp3"}},
           "futureField": {"x": [1, 2.5, None]},
           "sections": [
               {"id": "s1", "title": "Arrival", "layout": "media-top", "slide": True,
                "media": [{"libraryId": "main", "path": "trips/iceland/reykjavik.jpg", "caption": "Reykjavík harbour", "crop": {"x": 0.1}}],
                "text": {"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "We landed in the rain."}]}]},
                "notes": "mention the weather", "chapter": True},
               {"id": "s2", "title": "Drive", "layout": "gallery", "slide": True, "gallery": {"mode": "grid", "columns": 3},
                "media": [{"libraryId": "main", "path": "trips/iceland/drive.mp4", "startAt": 1.5, "muted": True, "loop": True},
                          {"libraryId": "main", "path": "Смешни/котка.gif"},
                          {"libraryId": "main", "path": "old.mkv"}]},
               {"id": "s1", "layout": "bogus", "media": [], "canvas": {"x": 0, "y": 0}},
           ],
           "canvas": {"items": [{"id": "n1", "type": "note", "text": "hi"}]}}
    both("PUT", "/api/documents/{}", {"baseVersion": 1, "document": doc})
    both("PUT", "/api/documents/{}", {"baseVersion": 1, "document": doc})  # conflict 409
    both("PUT", "/api/documents/{}", {"baseVersion": 2, "document": {**doc, "sections": [{"media": [{"libraryId": "main", "path": "../x"}]}]}})
    both("PUT", "/api/documents/{}", {"baseVersion": 2, "document": {**doc, "canvas": [1, 2]}})
    both("PUT", "/api/documents/{}", {"baseVersion": 2})
    gd, rd = both("GET", "/api/documents/{}")
    both("POST", "/api/documents/{}/snapshot")
    both("PUT", "/api/documents/{}", {"baseVersion": 2, "document": {**doc, "title": "Trip v3"}})
    # sizes of history snapshots differ by the digits of their (different) timestamps
    both("GET", "/api/documents/{}/history", tf=lambda j: [dict(e, bytes=e["bytes"] > 0) for e in j])
    both("GET", "/api/documents/{}/history/2")
    both("GET", "/api/documents/{}/history/99")
    both("POST", "/api/documents/{}/history/2/restore")
    both("GET", "/api/documents/{}")
    compare(go, rs, "GET", "/api/documents")
    compare(go, rs, "GET", "/api/search?q=rain")
    compare(go, rs, "GET", "/api/search?q=ICELAND")
    compare(go, rs, "GET", "/api/search?q=")
    compare(go, rs, "GET", "/api/usage?library=main&path=trips/iceland/drive.mp4")
    compare(go, rs, "GET", "/api/usage?library=main&path=trips&dir=1")
    compare(go, rs, "GET", "/api/usage?library=main&path=music/tone.mp3")
    for c, i in ((go, ids[0]), (rs, ids[1])):
        s, h, body = c.req("GET", f"/api/documents/{i}/export", raw=True)
        assert s == 200 and h["Content-Type"] == "application/json", (c.name, s, dict(h))
        print(f"export {c.name}: {h['Content-Disposition']}")
        exported = json.loads(body)
        s, imp = c.req("POST", "/api/documents/import", exported)
        assert s == 201, (c.name, s, imp)
    compare(go, rs, "POST", "/api/documents/import", "[1,2]")
    dup = both("POST", "/api/documents/{}/duplicate")
    compare(go, rs, "GET", "/api/documents")
    compare(go, rs, "GET", "/api/documents/not-a-valid-id")
    zips = []
    for c in (go, rs):
        s, h, body = c.req("GET", "/api/export", raw=True)
        z = zipfile.ZipFile(io.BytesIO(body))
        assert z.testzip() is None
        zips.append(sorted(n.split("/")[0] + "/" + ("<hist>" if "/.history/" in n else "<doc>") for n in z.namelist()))
        print(f"backup {c.name}: {h['Content-Disposition']} {len(z.namelist())} entries")
    diff(zips[0], zips[1], "GET /api/export (zip entries)")
    for c, i in ((go, ids[0]), (rs, ids[1])):
        s, h, body = c.req("POST", f"/api/documents/{i}/offline",
                           urllib.parse.urlencode({"payload": json.dumps({"html": {"s1": "<p>hi</p>"}})}),
                           headers={"Content-Type": "application/x-www-form-urlencoded"}, raw=True)
        z = zipfile.ZipFile(io.BytesIO(body))
        assert z.testzip() is None
        names = sorted(n.split("/", 1)[1] for n in z.namelist())
        data = z.read(z.namelist()[1] if "data.js" in z.namelist()[1] else [n for n in z.namelist() if n.endswith("data.js")][0])
        slides = json.loads(data.decode()[len("window.LETOPIS = "):-2])
        zips.append((names, norm(slides)))
        print(f"offline {c.name}: {s} {h['Content-Disposition']} {names}")
    diff(zips[2], zips[3], "POST /api/documents/{id}/offline (zip)")
    both("DELETE", "/api/documents/{}")
    both("DELETE", "/api/documents/{}")
    compare(go, rs, "GET", "/api/documents")

    # ---------- media ----------
    def hdrs(c, path, h=None, method="GET"):
        s, hd, body = c.req(method, path, headers=h, raw=True)
        return s, {k.lower(): v for k, v in hd.items()}, body
    keep = ["content-type", "content-range", "content-length", "accept-ranges", "cache-control", "etag",
            "last-modified", "content-disposition", "x-content-type-options", "x-frame-options", "referrer-policy"]
    for path, h in [
        ("/media/main/trips/iceland/drive.mp4", {"Range": "bytes=0-99"}),
        ("/media/main/trips/iceland/drive.mp4", {"Range": "bytes=-10"}),
        ("/media/main/trips/iceland/drive.mp4", {"Range": "bytes=99999999-"}),
        ("/media/main/trips/iceland/drive.mp4", {}),
        ("/media/main/trips/iceland/drive.mp4?download=1", {}),
        ("/media/main/%D0%A1%D0%BC%D0%B5%D1%88%D0%BD%D0%B8/%D0%BA%D0%BE%D1%82%D0%BA%D0%B0.gif", {}),
        ("/media/main/notes.txt", {}),
        ("/media/main/.hidden/secret.webp", {}),
        ("/media/main/../etc/passwd", {}),
        ("/media/nope/a.jpg", {}),
    ]:
        sg, hg, bg = hdrs(go, path, h)
        sr, hr, br = hdrs(rs, path, h)
        where = f"GET {path} {h}"
        if sg != sr:
            DIFFS.append((where, "status", sg, sr))
        for k in keep:
            if hg.get(k) != hr.get(k):
                DIFFS.append((where, "header " + k, hg.get(k), hr.get(k)))
        if sg in (200, 206) and bg != br:
            DIFFS.append((where, "body", len(bg), len(br)))
    # conditional request
    _, h0, _ = hdrs(rs, "/media/main/trips/iceland/drive.mp4")
    s, _, _ = hdrs(rs, "/media/main/trips/iceland/drive.mp4", {"If-None-Match": h0["etag"]})
    if s != 304:
        DIFFS.append(("rs If-None-Match", "status", 304, s))

    by = {f["path"]: f for f in all_rs["items"]}
    derived = [
        (by["trips/iceland/reykjavik.jpg"]["key"], "t320.webp"),
        (by["trips/iceland/reykjavik.jpg"]["key"], "t1280.webp"),
        (by["trips/iceland/reykjavik.jpg"]["key"], "t999.webp"),
        (by["trips/fractal.png"]["key"], "t2560.webp"),
        (by["trips/iceland/drive.mp4"]["key"], "t320.webp"),
        (by["trips/iceland/drive.mp4"]["key"], "sprite.jpg"),
        (by["Смешни/котка.gif"]["key"], "gif.mp4"),
        (by["live/IMG_0001.jpg"]["key"], "motion.mp4"),
        (by["music/tone.mp3"]["key"], "t320.webp"),
        (by["old.mkv"]["key"], "full.mp4"),
        ("0123456789abcdef0123", "t320.webp"),
        ("short", "t320.webp"),
    ]
    for key, name in derived:
        p = f"/d/{key}/{name}"
        sg, hg, bg = hdrs(go, p)
        sr, hr, br = hdrs(rs, p)
        where = f"GET {p}"
        if sg != sr:
            DIFFS.append((where, "status", sg, sr))
        for k in ["content-type", "cache-control"]:
            if hg.get(k) != hr.get(k):
                DIFFS.append((where, "header " + k, hg.get(k), hr.get(k)))
        print(f"{name:12} go={sg} {len(bg):7}B  rs={sr} {len(br):7}B  {hr.get('content-type')}")
    # proxies: request one for the MKV (not playable) and wait
    mkv = by["old.mkv"]
    compare(go, rs, "POST", "/api/proxy", {"key": mkv["key"], "variant": "bogus"})
    compare(go, rs, "POST", "/api/proxy", {"key": "nope", "variant": "full"})
    for c in (go, rs):
        k = {f["path"]: f for f in (all_go if c is go else all_rs)["items"]}["old.mkv"]["key"]
        for _ in range(120):
            s, st = c.req("POST", "/api/proxy", {"key": k, "variant": "full"})
            if st.get("status") in ("ready", "failed"):
                break
            time.sleep(1)
        s, h, body = hdrs(c, f"/d/{k}/full.mp4", {"Range": "bytes=0-1023"})
        print(f"proxy {c.name}: {st} → {s} {h.get('content-range')}")
        if st.get("status") != "ready" or s != 206:
            DIFFS.append(("proxy " + c.name, "status", "ready/206", f"{st}/{s}"))
    compare(go, rs, "POST", "/api/resolve", {"refs": [{"libraryId": "main", "path": "old.mkv"}]})
    compare(go, rs, "POST", "/api/prewarm", {"refs": refs[:5]})

    # ---------- web UI ----------
    for c in (go, rs):
        s, h, body = hdrs(c, "/")
        s2, h2, body2 = hdrs(c, "/some/spa/route")
        html = body.decode(errors="replace")
        import re
        assets = re.findall(r'(?:src|href)="(/assets/[^"]+)"', html)
        ok = s == 200 and "<html" in html.lower() and body2 == body and assets
        astat = [hdrs(c, a)[0] for a in assets]
        print(f"web {c.name}: / {s} {h.get('content-type')} cache={h.get('cache-control')} assets={astat} "
              f"asset-cache={hdrs(c, assets[0])[1].get('cache-control') if assets else None}")
        if not ok or any(x != 200 for x in astat):
            DIFFS.append(("web " + c.name, "index/assets", "ok", f"{s} {astat}"))
    def shape(j):
        if isinstance(j, dict):
            return {k: shape(v) for k, v in j.items() if k not in ("ffmpeg", "tools")}
        if isinstance(j, list):
            return [shape(j[0])] if j else []
        if isinstance(j, bool):
            return "bool"
        if isinstance(j, (int, float)):
            return "number"
        return type(j).__name__
    compare(go, rs, "GET", "/api/status", transform=shape)
    _, st = rs.get("/api/status")
    print("rust status tools:", st.get("tools"), "hw:", st.get("hw"))
    compare(go, rs, "GET", "/api/nope")
    compare(go, rs, "GET", "/healthz", status_only=True)
    compare(go, rs, "POST", "/api/logout", {})
    compare(go, rs, "GET", "/api/libraries")

    # ---------- report ----------
    print()
    allowed = [d for d in DIFFS if any(d[0].startswith(w) and d[1] == p for w, p in ALLOWED)]
    for d in allowed:
        print(f"allowed difference: {d[0]} {d[1]}: go={d[2]!r} rs={d[3]!r}")
        DIFFS.remove(d)
    if not DIFFS:
        print("NO DIFFERENCES")
        return 0
    print(f"{len(DIFFS)} DIFFERENCES:")
    for where, path, a, b in DIFFS:
        sa, sb = json.dumps(a, ensure_ascii=False)[:300], json.dumps(b, ensure_ascii=False)[:300]
        print(f"- {where} {path}\n    go: {sa}\n    rs: {sb}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
