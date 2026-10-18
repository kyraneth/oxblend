#!/usr/bin/env bash
# No-network / no-telemetry verification for the qualified tree.
#
# 1. The locked graph has no HTTP client, TLS, DNS or async-runtime crate.
# 2. The engine source has no socket, DNS or HTTP call and no telemetry client.
#
# URL literals are reported rather than failed: the only ones in src/ are the
# xxHash licence header and the remote pack-target prefixes that pack refuses.
set -uo pipefail
cd "$(dirname "$0")/.."

network='^(reqwest|hyper|hyper-util|hyper-rustls|ureq|curl|curl-sys|isahc|surf|attohttpc|awc|tokio|tokio-util|async-std|smol|mio|async-io|native-tls|rustls|openssl|openssl-sys|webpki|webpki-roots|trust-dns|hickory-proto|hickory-resolver|dns-lookup|socket2|tungstenite)$'
calls='std::net|TcpStream|UdpSocket|UnixStream|libc::socket|libc::connect|libc::accept|libc::getaddrinfo|libc::sendto|to_socket_addrs|reqwest|ureq|tungstenite'
telemetry='telemetry|analytics|sentry|posthog|mixpanel|amplitude|segment\.io|matomo|google.analytics|crashpad|breakpad'

fail=0

hits=$(grep -E '^name = "' Cargo.lock | sed 's/^name = "//; s/"$//' | grep -nE "$network" || true)
if [ -n "$hits" ]; then
    printf 'no-network: network crate in Cargo.lock:\n%s\n' "$hits"
    fail=1
fi

hits=$(grep -rnE "$calls" src/ --include='*.rs' || true)
if [ -n "$hits" ]; then
    printf 'no-network: network call in src/:\n%s\n' "$hits"
    fail=1
fi

hits=$(grep -rniE "$telemetry" src/ Cargo.toml Cargo.lock || true)
if [ -n "$hits" ]; then
    printf 'no-telemetry: telemetry identifier found:\n%s\n' "$hits"
    fail=1
fi

urls=$(grep -rn 'https\?://' src/ --include='*.rs' | wc -l)
printf 'no-network: %s URL literal line(s) in src/ (licence header and refused remote pack targets)\n' "$urls"

[ "$fail" -eq 0 ] || exit 1
printf 'NO-NETWORK EXIT=0\n'
