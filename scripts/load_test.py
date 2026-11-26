#!/usr/bin/env python3
"""Нагрузочный тест NeXXuS node: N параллельных клиентов делают upload/download,
сверяют sha256, меряют latency/throughput, в конце снимают node_metrics/node_status.

Чистый stdlib (socket+threading), как e2e_smoke.py.

Пример:
  python3 scripts/load_test.py --api 127.0.0.1:4010 --clients 50 --size-mib 1 --rounds 3
  python3 scripts/load_test.py --api 10.0.0.5:4010 --clients 200 --size-mib 4 --rounds 2 --out report.json
"""
import argparse, base64, hashlib, json, os, socket, struct, sys, tempfile, threading, time

def ws_connect(host, port, timeout=60):
    s = socket.create_connection((host, port), timeout=timeout)
    key = base64.b64encode(os.urandom(16)).decode()
    s.sendall((f"GET / HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: Upgrade\r\n"
               f"Upgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {key}\r\n\r\n").encode())
    resp = b""
    while b"\r\n\r\n" not in resp:
        chunk = s.recv(4096)
        if not chunk:
            raise RuntimeError("WS handshake failed")
        resp += chunk
    assert b"101" in resp.split(b"\r\n")[0], resp[:200]
    return s

def send_text(s, text):
    p = text.encode(); mask = os.urandom(4); n = len(p)
    h = bytearray([0x81])
    if n < 126: h.append(0x80 | n)
    elif n < 65536: h.append(0x80 | 126); h += struct.pack(">H", n)
    else: h.append(0x80 | 127); h += struct.pack(">Q", n)
    h += mask
    s.sendall(bytes(h) + bytes(b ^ mask[i % 4] for i, b in enumerate(p)))

def recv_text(s):
    h = s.recv(2)
    if len(h) < 2: raise RuntimeError("WS closed")
    ln = h[1] & 0x7F
    if ln == 126: ln = struct.unpack(">H", s.recv(2))[0]
    elif ln == 127: ln = struct.unpack(">Q", s.recv(8))[0]
    buf = b""
    while len(buf) < ln:
        buf += s.recv(ln - len(buf))
    return buf.decode()

class Api:
    """Один запрос = одно WS-соединение: сервер закрывает сокет после ответа
    (local_api.rs: 'закрываем WebSocket соединение после ответа')."""
    def __init__(self, host, port, token):
        self.host, self.port, self.token = host, port, token
    def call(self, method, params=None):
        s = ws_connect(self.host, self.port)
        try:
            if self.token:
                send_text(s, json.dumps({"jsonrpc": "2.0", "method": "auth",
                                         "params": {"token": self.token}, "id": 0}))
                r = json.loads(recv_text(s))
                assert not r.get("error"), f"auth failed: {r}"
            send_text(s, json.dumps({"jsonrpc": "2.0", "method": method,
                                     "params": params or {}, "id": 1}))
            return json.loads(recv_text(s))
        finally:
            try: s.close()
            except Exception: pass

def worker(idx, args, api, results):
    # MAX_WS_CONNECTIONS на ноде = 16; держим запас
    sem = args._sem
    try:
        lat = []
        for rnd in range(args.rounds):
            size = args.size_mib * 1024 * 1024
            data = os.urandom(size)
            src = os.path.join(tempfile.gettempdir(), f"nexxus-load-{idx}-{rnd}.bin")
            with open(src, "wb") as f: f.write(data)
            want = hashlib.sha256(data).hexdigest()
            t0 = time.perf_counter()
            with sem: r = api.call("node_upload", {"path": src})
            t1 = time.perf_counter()
            if r.get("error"): raise RuntimeError(f"upload: {r['error']}")
            root = r["result"].get("root") or r["result"].get("hash")
            out = src + ".out"
            with sem: r = api.call("node_download", {"root": root, "out": out})
            t2 = time.perf_counter()
            if r.get("error"): raise RuntimeError(f"download: {r['error']}")
            got = hashlib.sha256(open(out, "rb").read()).hexdigest()
            assert got == want, f"sha mismatch: {got} != {want}"
            os.unlink(src); os.unlink(out)
            lat.append((t1 - t0, t2 - t1))
        results[idx] = {"ok": True, "lat": lat}
    except Exception as e:
        results[idx] = {"ok": False, "err": str(e)}

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--api", default="127.0.0.1:4010")
    ap.add_argument("--clients", type=int, default=20)
    ap.add_argument("--size-mib", type=int, default=1)
    ap.add_argument("--rounds", type=int, default=2)
    ap.add_argument("--out", default=None, help="JSON-отчёт")
    args = ap.parse_args()
    host, port = args.api.rsplit(":", 1)
    args.host, args.port = host, int(port)

    tok_path = os.path.expanduser("~/.nexxus/api.token")
    token = open(tok_path).read().strip() if os.path.exists(tok_path) else None

    results = {}
    api = Api(args.host, args.port, token)
    args._sem = threading.Semaphore(12)  # MAX_WS_CONNECTIONS=16 на ноде, запас 4
    threads = [threading.Thread(target=worker, args=(i, args, api, results))
               for i in range(args.clients)]
    t0 = time.perf_counter()
    for t in threads: t.start()
    for t in threads: t.join()
    wall = time.perf_counter() - t0

    ok = sum(1 for r in results.values() if r["ok"])
    ups, downs = [], []
    for r in results.values():
        if r["ok"]:
            for u, d in r["lat"]:
                ups.append(u); downs.append(d)
    def pct(v, p):
        if not v: return None
        v = sorted(v); return v[min(len(v) - 1, int(len(v) * p))]
    total_bytes = ok * args.rounds * args.size_mib * 1024 * 1024
    report = {
        "api": args.api, "clients": args.clients, "rounds": args.rounds,
        "size_mib": args.size_mib, "wall_s": round(wall, 2),
        "ok": ok, "fail": args.clients - ok,
        "errors": [r["err"] for r in results.values() if not r["ok"]][:5],
        "upload_s": {"p50": pct(ups, .5), "p95": pct(ups, .95), "max": max(ups) if ups else None},
        "download_s": {"p50": pct(downs, .5), "p95": pct(downs, .95), "max": max(downs) if downs else None},
        "throughput_mib_s": round(total_bytes / wall / 1048576, 2) if wall > 0 else None,
    }
    # метрики ноды после нагрузки
    try:
        api = Api(host, args.port, token)
        report["node_metrics"] = api.call("node_metrics").get("result")
        report["node_status"] = api.call("node_status").get("result")
    except Exception as e:
        report["metrics_error"] = str(e)
    print(json.dumps(report, indent=2, ensure_ascii=False))
    if args.out:
        with open(args.out, "w") as f: json.dump(report, f, indent=2, ensure_ascii=False)
    sys.exit(0 if ok == args.clients else 1)

if __name__ == "__main__":
    main()
