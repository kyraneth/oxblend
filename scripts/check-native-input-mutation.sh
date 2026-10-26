#!/usr/bin/env bash
# Seed a real source omission, not a JSON/seq corruption or alternate writer.
set -eu -o pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
: "${FLATTRACE_CHECKOUT:?set FLATTRACE_CHECKOUT to the unchanged flattrace 0.1 checkout}"
E=$(mktemp -d "${TMPDIR:-/tmp}/oxblend-fetch-mutation.XXXXXX")
printf 'evidence: %s\n' "$E"
# Never replace the positive executable. Reuse a caller's private build cache
# only in a distinct sibling target; otherwise keep it inside this evidence.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:+${CARGO_TARGET_DIR%/}-fetch-observer-mutant}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$E/target}"
FIX=$(git -C "$ROOT" rev-parse HEAD)
mkdir "$E/source"
git -C "$ROOT" archive "$FIX" | tar -x -C "$E/source"
python3 - "$E/source/src/fetch.rs" "$FIX" "$E" <<'PY'
import hashlib,json,pathlib,sys
path=pathlib.Path(sys.argv[1]);before=path.read_text()
old='"input":crate::diagnostics::observe_input(path)'
new='"input":crate::diagnostics::input_identity(path)'
assert before.count(old)==1, 'fetch observation owner drifted; inspect before updating the mutation'
path.write_text(before.replace(old,new))
(pathlib.Path(sys.argv[3])/'mutation.json').write_text(json.dumps(dict(source_commit=sys.argv[2],mutation='Remove fetch observe_input invocation, preserving actual capture and file.open identity payload',before_sha256=hashlib.sha256(before.encode()).hexdigest(),after_sha256=hashlib.sha256(path.read_bytes()).hexdigest()),indent=2))
PY
export CARGO_NET_OFFLINE=true
export OXBLEND_NATIVE_INPUT_EVIDENCE="$E/native"
export OXBLEND_NATIVE_INPUT_GATE=1
export OXBLEND_BUILD_COMMIT="$FIX+mutant-fetch-observation-omitted"
export PYTHONDONTWRITEBYTECODE=1
unset FLATTRACE_DIR FLATTRACE_TRACEPARENT FLATTRACE_LEVEL
set +e
cargo test --manifest-path "$E/source/Cargo.toml" --release --offline --locked --test native_input_facts -- --nocapture > "$E/native-test.log" 2>&1
rc=$?
set -e
printf '%s\n' "$rc" > "$E/native-test.exit"
python3 - "$E" "$E/source" "$rc" <<'PY'
import json,os,pathlib,subprocess,sys
E=pathlib.Path(sys.argv[1]);root=pathlib.Path(sys.argv[2]);rc=int(sys.argv[3]);log=(E/'native-test.log').read_text()
assert rc==101 and 'native fetch semantic gate failed' in log and 'missing captured input identity' in log,log
selections=list((E/'native').glob('*/fetch.selection.json'));assert len(selections)==1,selections
selection=selections[0];paths=list((selection.parent/'fetch-on').glob('*.jsonl'));assert len(paths)==1,paths
trace=paths[0];rows=[json.loads(s) for s in trace.read_text().splitlines()]
assert rows[0]['data']['command']=='library' and rows[0]['data']['argv']==[] and rows[0]['data']['inputs']==[]
summary=next(r for r in rows if r['event']=='root.summary' and r['data']['summary_phase']=='final')
assert summary['data']['inputs']==[]
assert any(r['event']=='fetch' and r['state']=='end' and r['outcome']=='ok' for r in rows)
assert any(r['event']=='file.open' and r['state']=='start' and r['data']['input']['signature'] for r in rows)
env=os.environ.copy();env['PYTHONPATH']=os.environ['FLATTRACE_CHECKOUT']
reference=subprocess.run(['python3','-m','flattrace','check',str(trace)],env=env,capture_output=True)
(E/'reference.stdout').write_bytes(reference.stdout);(E/'reference.stderr').write_bytes(reference.stderr)
assert reference.returncode==0,(reference.stdout,reference.stderr)
gate=subprocess.run(['python3',str(root/'scripts/check-flattrace.py'),str(trace),'--selection',str(selection)],env=env,capture_output=True)
(E/'gate.stdout').write_bytes(gate.stdout);(E/'gate.stderr').write_bytes(gate.stderr)
assert gate.returncode==1 and b'missing captured input identity' in gate.stderr,(gate.stdout,gate.stderr)
(E/'proof.json').write_text(json.dumps(dict(source_commit=json.loads((E/'mutation.json').read_text())['source_commit'],source_mutation_rejected=True,native_test_exit=rc,reference_exit=reference.returncode,semantic_gate_exit=gate.returncode,root_requested_inputs=rows[0]['data']['inputs'],root_actual_inputs=summary['data']['inputs'],fetch_capture_succeeded=True,identity_payload_present=True,trace=str(trace),selection=str(selection)),indent=2))
print(json.dumps(dict(source_mutation_rejected=True,reference_exit=0,semantic_gate_exit=1,evidence=str(E))))
PY
