#!/usr/bin/env python3
"""Optional Certbot DNS-01 hook. The token file is root-readable and outside git."""
import os
import pathlib
import sys
import time
import urllib.parse
import urllib.request

domain = os.environ["CERTBOT_DOMAIN"]
if not domain.endswith(".duckdns.org") or domain.count(".") != 2:
    raise SystemExit("Only a single DuckDNS subdomain is supported")
token = pathlib.Path(os.environ.get("SAFEEAT_DUCKDNS_TOKEN_FILE", "/etc/safeeat/duckdns-token")).read_text().strip()
cleanup = "--cleanup" in sys.argv
params = {"domains": domain.removesuffix(".duckdns.org"), "token": token, "txt": os.environ["CERTBOT_VALIDATION"]}
if cleanup:
    params["clear"] = "true"
request = urllib.request.Request("https://www.duckdns.org/update?" + urllib.parse.urlencode(params), method="GET")
try:
    with urllib.request.urlopen(request, timeout=20) as response:
        result = response.read().decode().strip()
except Exception:
    raise SystemExit("DuckDNS request failed; credential withheld") from None
if result != "OK":
    raise SystemExit("DuckDNS did not authorize this domain update")
if not cleanup:
    time.sleep(45)
