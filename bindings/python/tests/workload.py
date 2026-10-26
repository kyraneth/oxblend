"""One observed two-get task, or a syscall-count probe; installed wheel only."""
import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import resource
import subprocess
import sys
import time
import oxblend as ox
from flattrace import Writer

p = argparse.ArgumentParser()
p.add_argument("mode", choices=("native", "subprocess"))
p.add_argument("--path", required=True)
p.add_argument("--block", type=int, required=True)
p.add_argument("--cli", required=True)
p.add_argument("--output", type=Path, required=True)
a = p.parse_args()
w = Writer.from_env("oxblend-python-workload", ox.__version__, argv=sys.argv)
assert w is not None, "FLATTRACE_DIR required"
@contextmanager
def call(operation):
    token = w.start("python.api" if a.mode == "native" else "cli", {"operation": operation})
    try: yield
    finally: w.end(token, "ok")

def environment():
    mem = dict(line.rstrip().split(":", 1) for line in Path("/proc/meminfo").read_text().splitlines())
    return {"loadavg": os.getloadavg(), "mem_available": mem["MemAvailable"].strip(), "swap_free": mem["SwapFree"].strip()}

before = environment()
started = time.perf_counter()
if a.mode == "native":
    with call("open"): f = ox.open(a.path)
    try:
        with call("blocks"): block = next(b for b in f.blocks(code=b"SC") if b.index == a.block)
        values = []
        for field in (b"sfra", b"efra"):
            with call("get"): values.append(f.get([ox.FieldQuery(block, 0, (b"r", field))])[0].value)
    finally:
        with call("close"): f.close()
else:
    values = []
    for field in ("sfra", "efra"):
        with call("get"):
            r = subprocess.run([a.cli, "get", a.path, str(a.block), json.dumps(["r", field])], capture_output=True, check=True)
        values.append(int(json.loads(r.stdout)["value"]["decimal"]))
elapsed = time.perf_counter() - started
record = {"mode": a.mode, "path": a.path, "block": a.block, "values": values, "seconds": elapsed,
          "self_peak_rss_kib": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
          "children_peak_rss_kib": resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss,
          "before": before, "after": environment(), "python": sys.version,
          "module": ox.__file__, "native_module": ox._native.__file__,
          "coverage": "One task, warm-cache uncontrolled loaded host; no general speed claim. Native includes blocks selection; CLI uses supplied index. Import/startup excluded from internal timer; use external time receipt for process total."}
a.output.write_text(json.dumps(record, indent=2))
w.close()
print(json.dumps(record))
