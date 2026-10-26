"""One installed native Session, one parse per captured lexical source.

Run at FLATTRACE_LEVEL=2; count snapshot/index/schema parent paths in the
actual native trace. No test instrumentation, CLI or subprocess here.
"""
from __future__ import annotations
import argparse
from pathlib import Path
import json
import resource
import sys
import oxblend as ox

p = argparse.ArgumentParser()
p.add_argument("--input", type=Path, required=True)
p.add_argument("--output", type=Path, required=True)
p.add_argument("--result", type=Path, required=True)
a = p.parse_args()
with ox.open(a.input) as f:
    blocks = f.blocks(code=b"SC")
    summary = f.summary()
    frame = summary.scenes[0].frame_start if summary.scenes else None
    if blocks:
        # The synthetic linked-library fixture deliberately omits RenderData;
        # use a field actually present rather than defaulting a frame value.
        if frame is None:
            assert f.get([ox.FieldQuery(blocks[0], 0, (b"id", b"name"))])[0].value == summary.scenes[0].name
        else:
            assert f.get([ox.FieldQuery(blocks[0], 0, (b"r", b"sfra"))])[0].value == frame
    first = f.trace(allow_incomplete=True)
    second = f.trace(allow_incomplete=True)
    assert first == second
    plan = f.pack(a.output, project_root=a.input.parent, dry_run=True, allow_incomplete=True)
    assert not a.output.exists()
    assert f.summary() == summary
    query = f.query(type="Scene", fields=["r.sfra"], allow_incomplete=True)
    explanation = f.explain("scene.render")
assert f.closed
result = {"python": sys.version, "module": ox.__file__, "native": ox._native.__file__,
          "input": str(a.input), "binding": ox.__version__, "engine": ox.engine_version,
          "frame_start": frame, "trace_complete": first.complete, "plan_complete": plan.complete,
          "query_complete": query.complete, "closed": f.closed,
          "max_rss_kib": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
          "rss_source": "getrusage(RUSAGE_SELF).ru_maxrss; Linux KiB; whole Python process peak"}
a.result.write_text(json.dumps(result, indent=2))
print(json.dumps(result))
