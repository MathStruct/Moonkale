#!/usr/bin/env python3
"""Hard-coded colours outside the theme (Milestone 18 phase 4.4; zero since spec 030).

Counts colour literals (`#abc`, `#aabbcc`, `rgb(…)`, `rgba(…)`, `hsl(…)`) in
the tracked stylesheets, except vendored CSS. Every colour is a token
(`var(--mk-…)`) defined in packages/shell/src/theme.rs. The count may only
shrink: it fails when it is above the number in tools/colors-max.txt, and
asks for the number to be lowered when it is below.

    tools/check-colors.py           # check
    tools/check-colors.py --files   # the count per file
"""
import os, re, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MAX = os.path.join(ROOT, "tools", "colors-max.txt")
HEX = re.compile(r"#[0-9a-fA-F]{3,8}\b|\b(?:rgba?|hsla?)\(")
# Spec 030: the colours are in packages/shell/src/theme.rs; no stylesheet is exempt.
THEME = set()
# Vendored or built from node_modules (their colours are overridden through tokens).
VENDORED = ("packages/code-view/vendor/", "site/", ".obsidian/", "packages/editors/markdown/assets/katex/",
            "packages/editors/terminal/assets/xterm.css", "packages/editors/markdown/assets/milkdown.css")


def counts():
    files = subprocess.run(["git", "ls-files", "*.css"], cwd=ROOT, check=True,
                           capture_output=True, text=True).stdout.split()
    out = {}
    for f in files:
        if f in THEME or f.startswith(VENDORED):
            continue
        path = os.path.join(ROOT, f)
        # git ls-files still lists tracked files deleted in the working tree.
        if not os.path.isfile(path):
            continue
        with open(path, encoding="utf-8", errors="ignore") as fh:
            n = len(HEX.findall(fh.read()))
        if n:
            out[f] = n
    return out


def main():
    per_file = counts()
    total = sum(per_file.values())
    if "--files" in sys.argv:
        for f, n in sorted(per_file.items(), key=lambda x: -x[1]):
            print(f"{n:5}  {f}")
    ceiling = int(open(MAX).read().split()[0])
    print(f"{total} hard-coded colour(s) outside the theme (ceiling {ceiling})")
    if total > ceiling:
        print("NEW hard-coded colours: use a theme token (theme.css) instead")
        return 1
    if total < ceiling:
        print(f"Fewer than the ceiling: lower tools/colors-max.txt to {total}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
