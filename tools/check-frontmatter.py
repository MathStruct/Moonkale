#!/usr/bin/env python3
"""Every vault page's front matter must parse as YAML.

Quartz stops the whole site build on one bad page (an unquoted `: ` in a
description is the usual cause), and the deploy only runs on pushes to master,
so this runs in CI's fast job, where it's seen on every push.
"""
import pathlib
import re
import sys

import yaml

root = pathlib.Path(__file__).resolve().parent.parent / "markdown"
bad = []
for page in sorted(root.rglob("*.md")):
    text = page.read_text(encoding="utf-8")
    match = re.match(r"---\n(.*?)\n---\n", text, re.S)
    if not match:
        continue
    try:
        yaml.safe_load(match.group(1))
    except yaml.YAMLError as error:
        first = str(error).splitlines()[0]
        bad.append(f"{page.relative_to(root.parent)}: {first} (quote the value)")
for line in bad:
    print(line)
print(f"front matter: {len(bad)} problem(s)")
sys.exit(1 if bad else 0)
