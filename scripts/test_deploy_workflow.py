#!/usr/bin/env python3
"""Guard the deploy workflow against GitHub Actions' expression-size limit (OMK-446).

GitHub refuses to load `.github/workflows/deploy.yml` when a single `script:`
scalar is >= 21,000 characters ("Exceeded max expression length 21000"). The
workflow then startup-fails on every push, so the deploy silently never runs —
`test` and `mergeable_state` do not catch this.

This test extracts each block `script: |` scalar without needing PyYAML and
fails if any is at or above the limit.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEPLOY = os.path.join(ROOT, ".github", "workflows", "deploy.yml")
LIMIT = 21000
SCRIPT_HEADER = re.compile(r"^(\s*)script:\s*\|[+-]?\s*$")


def extract_scripts(path):
    lines = open(path, encoding="utf-8").read().splitlines()
    scripts = []
    index = 0
    while index < len(lines):
        match = SCRIPT_HEADER.match(lines[index])
        if not match:
            index += 1
            continue
        base_indent = len(match.group(1))
        block = []
        index += 1
        while index < len(lines):
            line = lines[index]
            if line.strip() == "":
                block.append("")
                index += 1
                continue
            indent = len(line) - len(line.lstrip(" "))
            if indent <= base_indent:
                break
            block.append(line)
            index += 1
        indents = [len(line) - len(line.lstrip(" ")) for line in block if line.strip()]
        common = min(indents) if indents else 0
        scripts.append("\n".join(line[common:] if line.strip() else "" for line in block))
    return scripts


def main():
    if not os.path.isfile(DEPLOY):
        raise SystemExit("deploy workflow not found: %s" % DEPLOY)
    scripts = extract_scripts(DEPLOY)
    if not scripts:
        raise SystemExit("FAIL: no block `script: |` scalars found in deploy.yml")
    failed = False
    for number, script in enumerate(scripts, start=1):
        size = len(script)
        status = "OK" if size < LIMIT else "TOO LONG"
        print("script #%d: %d chars -> %s" % (number, size, status))
        if size >= LIMIT:
            failed = True
    if failed:
        raise SystemExit(
            "FAIL: a deploy `script:` scalar is >= %d chars; GitHub will reject the "
            "workflow (Exceeded max expression length %d). Split the step or move "
            "inline code into a checked-in file." % (LIMIT, LIMIT)
        )
    print("PASS: all %d deploy script scalars are under %d chars" % (len(scripts), LIMIT))


if __name__ == "__main__":
    main()
