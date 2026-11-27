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


def check_race_hardening(scripts):
    """Guard the OMK-702 container-recreation race hardening.

    The SSH deploy step must not run a bare `docker compose up -d`: when compose
    recreates the fixed-name `catan` container it can lose a transient
    `<id>_catan` container and exit 1, marking the deploy red and skipping the
    post-deploy hardening steps. The step must remove the stale container first,
    run `up` with `--remove-orphans`, bound the retries, and verify the container
    actually came up.
    """
    deploy_script = None
    for script in scripts:
        if "docker compose pull catan" in script:
            deploy_script = script
            break
    if deploy_script is None:
        raise SystemExit("FAIL: could not find the deploy script containing `docker compose pull catan`")

    problems = []
    code = "\n".join(
        line
        for line in deploy_script.splitlines()
        if not line.lstrip().startswith("#")
    )
    ups = [
        match.start()
        for match in re.finditer(r"docker compose up -d(?! --remove-orphans)", code)
    ]
    if ups:
        problems.append(
            "found %d unguarded `docker compose up -d` call(s) without --remove-orphans" % len(ups)
        )
    if "docker compose up -d --remove-orphans" not in code:
        problems.append("missing `docker compose up -d --remove-orphans`")

    rm_index = code.find("docker rm -f catan")
    if rm_index == -1:
        problems.append("missing `docker rm -f catan` to clear the stale container")
    up_index = code.find("docker compose up -d --remove-orphans")
    if rm_index != -1 and up_index != -1 and rm_index > up_index:
        problems.append("`docker rm -f catan` must run before the first `docker compose up`")

    for marker, description in (
        ("up_attempt", "bounded retry counter"),
        ("up_ok", "retry success flag"),
    ):
        if marker not in code:
            problems.append("missing %s (`%s`)" % (description, marker))

    if "docker inspect catan" not in code:
        problems.append("missing post-up `docker inspect catan` status check")

    if problems:
        raise SystemExit("FAIL: deploy race hardening incomplete:\n  - " + "\n  - ".join(problems))
    print("PASS: deploy step guards the container-recreation race (remove + bounded retry + status check)")


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
    check_race_hardening(scripts)


if __name__ == "__main__":
    main()
