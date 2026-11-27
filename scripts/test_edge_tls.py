#!/usr/bin/env python3
"""Guard the OMK-481 edge TLS hardening in the deploy workflow.

The deploy writes a Traefik dynamic file that narrows the TLS 1.2 cipher list to
AEAD suites. TLS 1.3 must stay untouched, so `maxVersion` must never appear, and
the deprecated TLS 1.2 CBC/SHA-1 suites must never be reintroduced.

This test reads the heredoc body out of `.github/workflows/deploy.yml` (no PyYAML
required) and asserts the invariant, so an accidental edit is caught in CI rather
than in production.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEPLOY = os.path.join(ROOT, ".github", "workflows", "deploy.yml")

HEREDOC_OPEN = re.compile(r"<<'([A-Z0-9_]+)'")
AEAD_SUITES = [
    "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
    "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
    "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256",
    "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384",
    "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256",
    "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256",
]
FORBIDDEN_SUBSTRINGS = [
    "_WITH_AES_128_CBC_SHA",
    "_WITH_AES_256_CBC_SHA",
    "_WITH_AES_128_CBC_SHA256",
    "_WITH_AES_256_CBC_SHA384",
    "_WITH_3DES_",
    "TLS_RSA_WITH_",
    "maxVersion",
]


def fail(message):
    print("FAIL: " + message)
    raise SystemExit(1)


def extract_heredoc(lines, marker):
    """Return the de-indented body of the heredoc that writes `marker`."""
    opener = re.compile(r"<<'" + re.escape(marker) + r"'")
    start = None
    for index, line in enumerate(lines):
        if opener.search(line):
            start = index + 1
            break
    if start is None:
        fail("no heredoc opening <<'%s' found in deploy.yml" % marker)

    body = []
    closed = False
    for line in lines[start:]:
        if line.strip() == marker:
            closed = True
            break
        body.append(line)
    if not closed:
        fail("heredoc <<'%s' is never closed" % marker)

    indents = [len(line) - len(line.lstrip(" ")) for line in body if line.strip()]
    common = min(indents) if indents else 0
    return [line[common:] if line.strip() else "" for line in body]


def main():
    if not os.path.isfile(DEPLOY):
        raise SystemExit("deploy workflow not found: %s" % DEPLOY)

    text = open(DEPLOY, encoding="utf-8").read()
    lines = text.splitlines()

    if "OMK-481" not in text:
        fail("the OMK-481 TLS hardening step is missing from deploy.yml")

    # Locate the heredoc that writes catan-tls.yml.
    tls_marker = None
    for line in lines:
        match = HEREDOC_OPEN.search(line)
        if match and "CATAN_TLS_FILE" in line:
            tls_marker = match.group(1)
            break
    if tls_marker is None:
        # The writer uses a variable for the path and a fixed marker; fall back to
        # the documented marker name.
        tls_marker = "CATAN_TLS_YML"
    body = extract_heredoc(lines, tls_marker)
    body_text = "\n".join(body)

    if "minVersion: VersionTLS12" not in body_text:
        fail("the TLS option does not pin minVersion: VersionTLS12")
    if "catan-aead:" not in body_text or "default:" not in body_text:
        fail("expected both the named `catan-aead` and the provider `default` option")

    for suite in AEAD_SUITES:
        if suite not in body_text:
            fail("missing AEAD suite %s" % suite)

    for forbidden in FORBIDDEN_SUBSTRINGS:
        if forbidden in body_text:
            fail("forbidden entry %r found in the TLS option (CBC/SHA-1 or a TLS 1.3 cap)" % forbidden)

    # TLS 1.3 must stay available: the file must not cap the max version.
    if re.search(r"^\s*maxVersion\s*:", body_text, re.MULTILINE):
        fail("maxVersion is set; TLS 1.3 would be restricted")

    print("PASS: OMK-481 TLS option pins TLS 1.2 AEAD suites and leaves TLS 1.3 intact")


if __name__ == "__main__":
    main()
