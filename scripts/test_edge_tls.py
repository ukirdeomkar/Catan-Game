#!/usr/bin/env python3
"""Guard the OMK-481 edge TLS hardening in the deploy workflow.

The deploy writes a Traefik dynamic file that narrows the TLS 1.2 cipher list to
AEAD suites. TLS 1.3 must stay untouched, so `maxVersion` must never appear, and
the deprecated TLS 1.2 CBC/SHA-1 suites must never be reintroduced.

This test reads the heredoc body out of `.github/workflows/deploy.yml` (no PyYAML
required), splits the `tls.options` mapping into per-option blocks, and asserts
the invariant on *each* option rather than only once across the whole file.
`default:` is the option that actually covers the pre-existing router, so a suite
dropped from it must fail even when `catan-aead:` still carries the full list.
"""

import os
import re

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEPLOY = os.path.join(ROOT, ".github", "workflows", "deploy.yml")

HEREDOC_OPEN = re.compile(r"<<'([A-Z0-9_]+)'")
REQUIRED_OPTIONS = ["catan-aead", "default"]
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


def split_options(body):
    """Split the de-indented heredoc body into {option_name: block lines}.

    The heredoc is a small, fixed YAML subset (`tls.options.<name>...`), so an
    indentation-aware split keeps the guard dependency-free on hosts without
    PyYAML while still validating each option separately.
    """
    options_indent = None
    option_indent = None
    options = {}
    current = None
    for line in body:
        stripped = line.strip()
        if not stripped:
            if current is not None:
                options[current].append("")
            continue
        indent = len(line) - len(line.lstrip(" "))
        if options_indent is None:
            if stripped == "options:":
                options_indent = indent
            continue
        if indent <= options_indent:
            current = None
            continue
        if stripped.endswith(":") and not stripped.startswith("-"):
            if option_indent is None:
                option_indent = indent
            if indent == option_indent:
                current = stripped[:-1].strip()
                options[current] = []
                continue
        if current is not None:
            options[current].append(line)
    return options


def check_option(name, block):
    """Assert the TLS 1.2 AEAD invariant on a single `tls.options` entry."""
    block_text = "\n".join(block)

    if not re.search(r"^\s*minVersion:\s*VersionTLS12\s*$", block_text, re.MULTILINE):
        fail("option %r does not pin minVersion: VersionTLS12" % name)

    for suite in AEAD_SUITES:
        if suite not in block_text:
            fail("option %r is missing AEAD suite %s" % (name, suite))

    for forbidden in FORBIDDEN_SUBSTRINGS:
        if forbidden in block_text:
            fail(
                "option %r contains forbidden entry %r (CBC/SHA-1 or a TLS 1.3 cap)"
                % (name, forbidden)
            )

    if re.search(r"^\s*maxVersion\s*:", block_text, re.MULTILINE):
        fail("option %r sets maxVersion; TLS 1.3 would be restricted" % name)


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

    options = split_options(body)
    if not options:
        fail("could not parse any tls.options entries from the heredoc")

    for name in REQUIRED_OPTIONS:
        if name not in options:
            fail("expected tls.options entry %r is missing" % name)
        check_option(name, options[name])

    print(
        "PASS: OMK-481 TLS options (%s) each pin TLS 1.2 AEAD suites and leave "
        "TLS 1.3 intact" % ", ".join(REQUIRED_OPTIONS)
    )


if __name__ == "__main__":
    main()
