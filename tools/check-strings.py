#!/usr/bin/env python3
"""UI strings (spec 030): every crate with a `locales/` directory has en.ftl,
de.ftl and zh-CN.ftl with the same message ids; every `t!(…, "id"…)` in the
crate's sources names an id in en.ftl; and no id goes unused.

    tools/check-strings.py            # check
    tools/check-strings.py --stats    # ids per crate
"""
import os, re, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LANGS = ["en", "de", "zh-CN"]
MSG = re.compile(r"^([a-zA-Z][a-zA-Z0-9_-]*)\s*=", re.M)
# t!(ws, L, "id" …) or tr(L, lang, "id", …)
USE = re.compile(r'\bt!\(\s*[^,]+,\s*[^,]+,\s*"([^"]+)"|\btr\(\s*[A-Z_]+\s*,\s*[^,]+,\s*"([^"]+)"|\btl!\(\s*[^,]+,\s*"([^"]+)"')


def main():
    dirs = []
    for dirpath, dirnames, files in os.walk(os.path.join(ROOT, "packages")):
        dirnames[:] = [d for d in dirnames if d not in ("target", "node_modules", ".git")]
        if dirpath.endswith("/locales") and "en.ftl" in files:
            dirs.append(os.path.relpath(os.path.join(dirpath, "en.ftl"), ROOT))
    dirs.sort()
    problems, stats = [], []
    for en in dirs:
        loc = os.path.dirname(en)
        crate = os.path.dirname(loc)
        ids = {}
        for lang in LANGS:
            p = os.path.join(ROOT, loc, f"{lang}.ftl")
            if not os.path.exists(p):
                problems.append(f"{crate}: no {lang}.ftl")
                ids[lang] = set()
                continue
            ids[lang] = set(MSG.findall(open(p, encoding="utf-8").read()))
        for lang in LANGS[1:]:
            for k in sorted(ids["en"] - ids[lang]):
                problems.append(f"{crate}: {lang}.ftl lacks `{k}`")
            for k in sorted(ids[lang] - ids["en"]):
                problems.append(f"{crate}: {lang}.ftl has `{k}`, en.ftl does not")
        used = set()
        for dirpath, _, files in os.walk(os.path.join(ROOT, crate, "src")):
            for f in files:
                if f.endswith(".rs"):
                    src = open(os.path.join(dirpath, f), encoding="utf-8").read()
                    # Tests and doc examples use their own tables.
                    src = src.split("#[cfg(test)]")[0]
                    src = "\n".join(l for l in src.splitlines() if not l.lstrip().startswith("//"))
                    for m in USE.finditer(src):
                        used.add(m.group(1) or m.group(2) or m.group(3))
        for k in sorted(used - ids["en"]):
            problems.append(f"{crate}: `{k}` is used but not in en.ftl")
        # Looked up by the shell's Extensions panel, by convention.
        used |= {"extension-name", "extension-description"} & ids["en"]
        for k in sorted(ids["en"] - used):
            problems.append(f"{crate}: `{k}` is in en.ftl but never used")
        stats.append((crate, len(ids["en"])))
    if "--stats" in sys.argv:
        for c, n in stats:
            print(f"{n:5}  {c}")
    for p in problems:
        print(p)
    print(f"{sum(n for _, n in stats)} strings in {len(stats)} crate(s), {len(problems)} problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
