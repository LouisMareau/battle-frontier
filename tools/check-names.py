#!/usr/bin/env python3
"""The originality guard: fails when a file names another game's brand.

The brands are kept as SHA-256 hashes (tools/forbidden-names.sha256), so the names never
appear in this repository. Each line is lowercased, its accents removed and split into
words; every run of one to three words, and every piece of a long word (for names glued
to others), is hashed and looked up.

    python3 tools/check-names.py [file...]       every tracked file when none is given
    python3 tools/check-names.py --hash "name"   the line to add to the list for a new name
"""
import hashlib
import re
import subprocess
import sys
import unicodedata
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LIST = ROOT / "tools" / "forbidden-names.sha256"
# the lengths of word pieces checked inside longer words
PIECES = range(5, 13)


def normalize(text):
    text = unicodedata.normalize("NFKD", text.lower())
    return "".join(c for c in text if not unicodedata.combining(c))


def words(line):
    return re.findall(r"[a-z0-9]+", normalize(line))


def digest(phrase):
    return hashlib.sha256(phrase.encode("utf-8")).hexdigest()


def candidates(ws):
    for i in range(len(ws)):
        for n in (1, 2, 3):
            if i + n <= len(ws):
                yield " ".join(ws[i:i + n])
        w = ws[i]
        for size in PIECES:
            for start in range(0, len(w) - size + 1):
                if size < len(w):
                    yield w[start:start + size]


def load():
    return {l.strip() for l in LIST.read_text(encoding="utf-8").splitlines() if l.strip() and not l.startswith("#")}


def tracked():
    out = subprocess.run(["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True).stdout.splitlines()
    return [ROOT / f for f in out if (ROOT / f) != LIST]


def main(argv):
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if argv[:1] == ["--hash"]:
        phrase = " ".join(words(" ".join(argv[1:])))
        print(digest(phrase))
        return 0
    hashes = load()
    files = [Path(a) for a in argv] if argv else tracked()
    found = 0
    for f in files:
        try:
            data = f.read_bytes()
        except OSError:
            continue
        if b"\0" in data:
            continue
        try:
            text = data.decode("utf-8")
        except UnicodeDecodeError:
            continue
        for n, line in enumerate(text.splitlines(), 1):
            if any(digest(c) in hashes for c in candidates(words(line))):
                found += 1
                name = f.relative_to(ROOT) if f.is_absolute() and ROOT in f.parents else f
                print(f"{name}:{n}: {line.strip()[:160]}")
    if found:
        print(f"{found} lines name another game's brand: Battle Frontier only takes original work.", file=sys.stderr)
        return 1
    print(f"no forbidden names in {len(files)} files")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
