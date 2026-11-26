#!/usr/bin/env python3
"""Testnet fan-out: прогон scripts/load_test.py на списке хостов по SSH,
сбор JSON-отчётов, агрегация в единую сводку для инвест-дока.

Чистый stdlib. Хосты — по одному на строку в файле (или --hosts h1,h2,...),
ssh-доступ по ключу, на каждом хосту nexxusd уже запущен (API 127.0.0.1:4010).

Пример:
  python3 scripts/testnet_fanout.py --hosts-file hosts.txt --clients 20 --size-mib 1 --rounds 3 --out bench-network.json
  # локальная проверка (один "хост" = localhost, без ssh):
  python3 scripts/testnet_fanout.py --hosts localhost --local --clients 20 --size-mib 1 --rounds 2 --out /tmp/bench-local.json
"""
import argparse, json, os, statistics, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
LOAD = os.path.join(HERE, "load_test.py")


def run_host(host, args, local):
    out = tempfile.NamedTemporaryFile(suffix=".json", delete=False).name
    cmd = [sys.executable, LOAD, "--api", f"127.0.0.1:{args.api_port}",
           "--clients", str(args.clients), "--size-mib", str(args.size_mib),
           "--rounds", str(args.rounds), "--out", out]
    if local:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=args.timeout)
    else:
        p = subprocess.run(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10",
                            host, " ".join(cmd)], capture_output=True, text=True, timeout=args.timeout)
    if p.returncode != 0:
        return {"host": host, "ok": False, "error": (p.stderr or p.stdout)[-400:]}
    if local:
        path = out
    else:
        path = out + f".{host}"
        subprocess.run(["scp", "-q", f"{host}:{out}", path], timeout=60)
        subprocess.run(["ssh", host, f"rm -f {out}"], timeout=30)
    try:
        with open(path) as f:
            rep = json.load(f)
        rep["host"] = host
        rep["host_ok"] = True
        os.unlink(path)
        return rep
    except Exception as e:
        return {"host": host, "host_ok": False, "error": f"report parse: {e}"}


def pct(v, p):
    if not v:
        return None
    s = sorted(v)
    return s[min(len(s) - 1, int(len(s) * p / 100))]


def aggregate(reports):
    ok = [r for r in reports if r.get("host_ok")]
    bad = [r for r in reports if not r.get("host_ok")]
    ups50, ups95, downs50, downs95, thr = [], [], [], [], []
    clients_ok = clients_fail = 0
    for r in ok:
        clients_ok += r.get("ok", 0)
        clients_fail += r.get("fail", 0)
        u, d = r.get("upload_s", {}), r.get("download_s", {})
        if u.get("p50") is not None:
            ups50.append(u["p50"]); ups95.append(u["p95"])
        if d.get("p50") is not None:
            downs50.append(d["p50"]); downs95.append(d["p95"])
        if r.get("throughput_mib_s"):
            thr.append(r["throughput_mib_s"])
    summary = {
        "hosts_total": len(reports), "hosts_ok": len(ok),
        "failed_hosts": [b["host"] for b in bad],
        "clients_ok": clients_ok, "clients_fail": clients_fail,
        # по хостам нельзя точно склеить перцентили — берём mean p50 и max p95 (худший хост)
        "upload_p50_mean_s": round(statistics.mean(ups50), 2) if ups50 else None,
        "upload_p95_max_s": round(max(ups95), 2) if ups95 else None,
        "download_p50_mean_s": round(statistics.mean(downs50), 2) if downs50 else None,
        "download_p95_max_s": round(max(downs95), 2) if downs95 else None,
        "throughput_sum_mibps": round(sum(thr), 2) if thr else None,
        "throughput_mean_mibps": round(statistics.mean(thr), 2) if thr else None,
    }
    return {"summary": summary, "per_host": reports}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--hosts", default=None, help="h1,h2,...")
    ap.add_argument("--hosts-file", default=None)
    ap.add_argument("--local", action="store_true", help="localhost без ssh (проверка)")
    ap.add_argument("--api-port", type=int, default=4010, help="порт WS API на каждом хосте")
    ap.add_argument("--clients", type=int, default=20)
    ap.add_argument("--size-mib", type=int, default=1)
    ap.add_argument("--rounds", type=int, default=3)
    ap.add_argument("--timeout", type=int, default=900)
    ap.add_argument("--out", default="bench-network.json")
    args = ap.parse_args()

    hosts = []
    if args.hosts:
        hosts += [h.strip() for h in args.hosts.split(",") if h.strip()]
    if args.hosts_file:
        with open(args.hosts_file) as f:
            hosts += [l.strip() for l in f if l.strip() and not l.startswith("#")]
    hosts = list(dict.fromkeys(hosts)) or ["localhost"]

    t0 = time.time()
    reports = [run_host(h, args, args.local) for h in hosts]
    agg = aggregate(reports)
    agg["elapsed_s"] = round(time.time() - t0, 1)
    agg["params"] = {"clients": args.clients, "size_mib": args.size_mib, "rounds": args.rounds}
    with open(args.out, "w") as f:
        json.dump(agg, f, indent=2, ensure_ascii=False)
    s = agg["summary"]
    print(f"hosts {s['hosts_ok']}/{s['hosts_total']} ok, clients {s['clients_ok']}/{s['clients_ok'] + s['clients_fail']} ok")
    print(f"upload p50(mean)/p95(max): {s['upload_p50_mean_s']}/{s['upload_p95_max_s']} s; "
          f"download p50/p95: {s['download_p50_mean_s']}/{s['download_p95_max_s']} s")
    print(f"throughput sum: {s['throughput_sum_mibps']} MiB/s, mean: {s['throughput_mean_mibps']} MiB/s")
    if s["failed_hosts"]:
        print("FAILED HOSTS:", ", ".join(s["failed_hosts"]))
    print(f"report: {args.out}")


if __name__ == "__main__":
    main()
