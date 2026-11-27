#!/usr/bin/env python3
"""Regression test for the Catan hexports route writer (OMK-446).

The deploy workflow embeds a Traefik file-provider writer that reuses the
service backing the playcatanou.duckdns.org router. A redirect router that also
matches playcatanou (our port-80 -> 443 redirect) must never be selected as the
upstream, or hexports.duckdns.org would be repointed at http://127.0.0.1 and
return 502 on the next deploy.

This test extracts the embedded writer from deploy.yml and runs it in the
"second deploy" state (catan-redirect.yml already present), for both the PyYAML
parser and the no-PyYAML fallback parser, asserting the reused service is the
real app service, not the redirect noop.
"""

import os
import re
import subprocess
import sys
import tempfile
import textwrap

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEPLOY = os.path.join(ROOT, ".github", "workflows", "deploy.yml")

PLAY = """http:
  routers:
    catan:
      rule: "Host(`playcatanou.duckdns.org`)"
      entryPoints: ["websecure"]
      service: catan
      tls:
        certResolver: duckdns
  services:
    catan:
      loadBalancer:
        servers:
          - url: "http://178.156.234.174:8080"
"""

REDIRECT = """http:
  routers:
    catan-http-redirect-play:
      rule: "Host(`playcatanou.duckdns.org`) && !PathPrefix(`/.well-known/acme-challenge/`)"
      entryPoints: ["web"]
      middlewares: ["catan-https-redirect"]
      service: catan-redirect-noop
    catan-http-redirect-hex:
      rule: "Host(`hexports.duckdns.org`) && !PathPrefix(`/.well-known/acme-challenge/`)"
      entryPoints: ["web"]
      middlewares: ["catan-https-redirect"]
      service: catan-redirect-noop
  middlewares:
    catan-https-redirect:
      redirectScheme:
        scheme: https
        permanent: true
  services:
    catan-redirect-noop:
      loadBalancer:
        servers:
          - url: "http://127.0.0.1"
"""


def extract_writer():
    with open(DEPLOY, encoding="utf-8") as handle:
        text = handle.read()
    match = re.search(r"<<'HEXPORTS_PY'\n(.*?)\n\s*HEXPORTS_PY", text, re.S)
    if not match:
        raise SystemExit("could not find the embedded HEXPORTS_PY writer in deploy.yml")
    return textwrap.dedent(match.group(1))


def run_case(writer_path, python_flags, label):
    with tempfile.TemporaryDirectory() as directory:
        with open(os.path.join(directory, "playcatanou.yml"), "w", encoding="utf-8") as handle:
            handle.write(PLAY)
        with open(os.path.join(directory, "catan-redirect.yml"), "w", encoding="utf-8") as handle:
            handle.write(REDIRECT)
        output = os.path.join(directory, "hexports.yml")
        proc = subprocess.run(
            [sys.executable, *python_flags, writer_path, directory, output],
            capture_output=True,
            text=True,
        )
        if proc.returncode != 0:
            raise SystemExit(
                "%s: writer failed (exit %d):\n%s\n%s"
                % (label, proc.returncode, proc.stdout, proc.stderr)
            )
        with open(output, encoding="utf-8") as handle:
            body = handle.read()
        if "catan-redirect-noop" in body or "127.0.0.1" in body:
            raise SystemExit(
                "%s: FAIL — hexports route reused the redirect noop:\n%s" % (label, body)
            )
        if "service: catan\n" not in body:
            raise SystemExit(
                "%s: FAIL — expected to reuse the real app service 'catan', got:\n%s"
                % (label, body)
            )
        print("%s: PASS" % label)


def main():
    writer_body = extract_writer()
    with tempfile.TemporaryDirectory() as working:
        writer = os.path.join(working, "write_hexports_route.py")
        with open(writer, "w", encoding="utf-8") as handle:
            handle.write(writer_body)
        run_case(writer, [], "PyYAML parser")
        # -S skips site-packages so `import yaml` fails and the scoped fallback parser runs.
        run_case(writer, ["-S"], "fallback parser (no PyYAML)")


if __name__ == "__main__":
    main()
