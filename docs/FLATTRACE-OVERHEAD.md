# G7 flattrace overhead — measured consumer cost

Source **0aec3b0**, flattrace 0.1 unchanged. Executable SHA-256
`a2c00bfd7a74b071a7ec03b857c16b263f362bfd7e88ca9b6474101ce7b4ba29`.

## Method and selection

120 actual invocations across 30 cases, AB then BA (two observations per arm,
order counterbalanced; not a confidence or precision guarantee). Nothing was
discarded. The approved selection considered the R4 120-input corpus and chose
the largest physical member of each **observed** compression family plus the
real shared-library splash root:

- raw: scrapyard.blend, 760,588,050 bytes;
- seekable zstd: blender-5.2-splash.blend, 141,369,967 bytes;
- shared-library root: Blender-3.4.blend, 559,573 physical bytes (the root's small
  size does not represent its transitive workload).

Those real inputs ran summary, recursive trace, JSON list and pack dry-run.
Every other small CLI command also ran, including actual edit/rewrite and fresh
directory/sequence packs. Actual gzip/error/archive/durability branches have
separate gate qualification; there was no gzip family in the selected corpus.
The workload names above identify the measured selection; private corpus
paths are not part of the public distribution.

Host: 16 logical CPUs, observed one-minute load **27.17–39.37**. This is a busy,
oversubscribed host. The noise flag below is data-derived: absolute median
on/off delta exceeds the larger observed within-arm range. It is **not**
statistical significance or proof of causation.

## Main result

Enabled diagnostics have a **real fixed cost on small CLI calls**: most flagged
small cases added about **10–26 ms** here. Help measured 2.003 ms off versus
14.489 ms on. Raw scrapyard list measured 494.432 ms off versus 633.807 ms on
(+139.375 ms, +28.2%); that fixture returns incomplete/EXIT 2 in both modes, so
this is the cost of the actual incomplete traversal, not a complete-pack claim.

The splash traversal's 40-second scale was dominated by product work and host
variation; measured deltas did not exceed its observed range. That is **not**
proof of zero tracing overhead. The apparent -226 ms pack improvement on
scrapyard was also flagged and profiled, but user CPU increased with tracing;
there is no intrinsic-speedup claim.

96 timed invocations exited 0 and 24 exited 2. Within every read/dry-run case,
stdout/stderr hashes and exits were identical across all four observations.
Actual small edit/rewrite/copied-pack Blend bytes also matched across arms
(`L5/output-byte-proof.json`); pack-info intentionally names each fresh target.
Off arms wrote **0 trace bytes**; on arms retained **140,075,653 bytes** total.

## Profile, before making the cost claim

All **19 flagged cases** were profiled in both modes using aggregate
`strace -f -c` (38 successful profiler executions; product exits preserved).
Fresh output targets prevented replay/overwrite. Full receipts and per-event,
inclusive trace timing profiles are in `L5/profile-allthreads/`; an earlier
main-thread-only profile is retained, not silently discarded.

The profile distinguishes two costs:

1. **Startup identity/writer setup.** A detailed help profile proves enabled
   startup reads all **7,187,672 executable bytes** before the run root. The
   adapter computes its actual SHA-256 and initializes the credential sanitizer
   and writer there. Help added about 7.6 ms user CPU in timed observations,
   while its profiled root residence was below 1 ms. Root-span residence does
   not include that startup, and these measurements are not an exact CPU
   allocation among hashing/sanitizer/scheduling. Source ownership plus the
   full-binary-read profile establishes the fixed initialization work.
2. **Per-event local serialization/IO.** Scrapyard list's all-thread profile
   went from 84 to 3,435 writes and added 3,352 getpid calls. Its trace contains
   3,084 explicit selection decisions plus resolve and other phases. User CPU
   rose from 141.1 to 189.2 ms in the timed observations. The syscall/profile
   evidence supports logging volume as a contributor; strace perturbs timing
   and its wall durations are not substituted for the AB/BA measurements.

Keep public/default tracing **off** and document the opt-in cost. There is no
near-zero disabled-overhead claim, no universal speed claim, no new performance
ceiling and no proposed upstream flattrace work. A future consumer-only lossless
aggregation optimization would need the same semantic gate and fresh paired
measurements; it is not hidden inside this delivery.

## Per-case medians, ranges, noise flags and load

| Case | Off median [min,max] ms | On median [min,max] ms | Delta ms | > observed noise | Load 1m range |
|---|---:|---:|---:|---|---|
| corpus-001-summary | 252.788 [239.681,265.894] | 247.643 [245.706,249.579] | -5.145 | False | 32.25–32.25 |
| corpus-001-trace | 40455.865 [39749.026,41162.703] | 40891.375 [40886.257,40896.493] | 435.510 | False | 27.17–32.25 |
| corpus-001-list | 43322.286 [41708.359,44936.212] | 43055.442 [41709.129,44401.755] | -266.844 | False | 27.39–37.11 |
| corpus-001-pack | 19873.723 [18695.947,21051.500] | 21327.856 [20409.220,22246.492] | 1454.133 | False | 34.91–39.37 |
| corpus-005-summary | 285.767 [276.291,295.243] | 302.264 [300.251,304.277] | 16.497 | False | 34.91–36.20 |
| corpus-005-trace | 548.963 [490.252,607.674] | 563.738 [556.862,570.615] | 14.776 | False | 36.20–36.20 |
| corpus-005-list | 494.432 [451.895,536.970] | 633.807 [609.007,658.608] | 139.375 | True | 34.50–36.20 |
| corpus-005-pack | 1686.088 [1634.734,1737.442] | 1459.852 [1446.266,1473.438] | -226.236 | True | 34.50–34.86 |
| corpus-071-summary | 2352.776 [2148.767,2556.785] | 2519.513 [2358.016,2681.009] | 166.737 | False | 34.63–34.95 |
| corpus-071-trace | 470.376 [465.616,475.137] | 508.995 [474.428,543.562] | 38.619 | False | 34.63–34.63 |
| corpus-071-list | 481.374 [454.719,508.029] | 470.276 [457.638,482.914] | -11.098 | False | 33.70–34.63 |
| corpus-071-pack | 565.485 [563.001,567.968] | 758.589 [600.679,916.499] | 193.104 | False | 33.70–33.70 |
| small-inspect | 99.877 [97.050,102.704] | 110.046 [105.883,114.209] | 10.169 | True | 33.70–33.70 |
| small-get | 11.535 [10.816,12.253] | 28.267 [27.904,28.629] | 16.732 | True | 33.70–33.70 |
| small-pointer | 14.161 [11.717,16.605] | 33.512 [32.119,34.905] | 19.351 | True | 33.70–33.70 |
| small-payload | 12.922 [11.858,13.987] | 30.361 [27.158,33.564] | 17.439 | True | 33.70–33.70 |
| small-source-facts | 10.880 [10.287,11.472] | 36.470 [30.690,42.251] | 25.591 | True | 33.70–33.70 |
| small-library-facts | 12.036 [11.264,12.808] | 26.726 [26.647,26.805] | 14.690 | True | 33.70–33.70 |
| small-blocks | 15.530 [11.648,19.412] | 30.277 [28.428,32.127] | 14.747 | True | 33.70–33.70 |
| small-overrides | 17.139 [13.037,21.241] | 30.488 [27.278,33.697] | 13.349 | True | 33.70–33.70 |
| small-pointer-census | 18.101 [13.088,23.114] | 28.439 [28.216,28.661] | 10.338 | True | 33.70–33.70 |
| small-diff | 23.917 [23.269,24.564] | 37.267 [37.150,37.384] | 13.351 | True | 33.70–33.70 |
| small-exr-header | 6.767 [3.695,9.839] | 31.800 [21.052,42.549] | 25.034 | True | 33.70–33.70 |
| small-help | 2.003 [1.992,2.014] | 14.489 [14.478,14.500] | 12.486 | True | 33.70–33.70 |
| small-version | 2.348 [1.807,2.888] | 42.227 [17.675,66.778] | 39.879 | False | 33.70–33.70 |
| small-skill | 2.601 [2.057,3.145] | 16.206 [14.858,17.554] | 13.605 | True | 33.70–33.70 |
| small-edit | 29.698 [23.039,36.357] | 45.608 [39.407,51.809] | 15.910 | True | 33.70–34.13 |
| small-rewrite | 12.380 [11.004,13.756] | 31.034 [30.498,31.569] | 18.653 | True | 34.13–34.13 |
| small-pack-execute | 18.297 [13.958,22.636] | 32.798 [28.944,36.653] | 14.501 | True | 34.13–34.13 |
| small-pack-sequence | 16.222 [16.207,16.237] | 35.797 [34.040,37.553] | 19.575 | True | 34.13–34.13 |
