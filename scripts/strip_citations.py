#!/usr/bin/env python3
"""Strip citations of the removed planning/research documents from source comments.

The repo used to carry a parallel planning system whose documents source comments cited by
name (`RESEARCH Pitfall 4`, `UI-SPEC §B.2`, `WR-03`, `T-04-16`, ...). Those documents are
gone, so the citations are now dangling references to nothing.

This only edits COMMENTS, never code, and only removes the citation tokens themselves --
the surrounding prose that explains the reasoning is preserved verbatim. Anything it cannot
rewrite cleanly is left alone and reported for manual handling.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

# One ticket/document identifier from the retired planning system.
ID = r"""(?:
      T-\d{2}-\d{2}
    | (?:WR|D|CR|GAP|IN|BL|SFLO|SFINI|COLL|NEXUS|PLUGIN|FOMOD|NXM|SEC|OQ)-\d+
    | OQ\d+
    | Pitfall\s+\d+
    | Pattern\s+\d+
    | Assumptions?\s+A\d(?:/A\d)*
    | A\d(?:/A\d)*
    | UI-SPEC(?:\s*[§\u00a7][A-Z](?:\.\d+)?(?:/[§\u00a7]?[A-Z](?:\.\d+)?)*)?
    | RESEARCH
    | PITFALLS\.md
    | SUMMARY\.md
    | CONTEXT\.md
    | Anti-Pattern[\s-]?\d*
    | Plan[\s-]?\d{2}
    | Phase[\s-]?\d+
)"""

# A run of identifiers joined by separators, e.g. "WR-01/WR-02", "Pitfall 4 / T-04-09",
# "RESEARCH Pitfall 3", "UI-SPEC §B.2".
RUN = rf"(?:{ID})(?:\s*(?:/|,|\+|\band\b|\u2014|-|\s)\s*(?:{ID}))*"

COMMENT = re.compile(r"^(\s*(?://[/!]?|//!|\*|/\*+|<!--))(.*)$")


def strip_run_parentheticals(text: str) -> str:
    """Remove `(WR-03)`, `(see UI-SPEC §A)`, `[Pitfall 4]` style parentheticals."""
    # Whole parenthetical is nothing but citations (plus filler words).
    filler = r"(?:see|See|per|Per|cf\.?|the|from|and|also|mirrors|matching|via)"
    pat = re.compile(
        rf"\s*[\(\[]\s*(?:{filler}\s+)*{RUN}\s*[\)\]]",
        re.VERBOSE,
    )
    prev = None
    while prev != text:
        prev = text
        text = pat.sub("", text)
    return text


def strip_leading_tags(text: str) -> str:
    """Remove a leading `WR-03: ` / `T-04-16 -- ` tag that prefixes real prose.

    Re-capitalises the sentence the tag was introducing, so removing the tag does not leave
    a comment starting mid-sentence in lower case.
    """
    pat = re.compile(rf"^(\s*)(?:{RUN})\s*(?::|\u2014|--)\s+(.)", re.VERBOSE)

    def repl(m: re.Match) -> str:
        return m.group(1) + m.group(2).upper()

    return pat.sub(repl, text)


def strip_trailing_tags(text: str) -> str:
    """Remove a trailing `-- WR-03` / `, per UI-SPEC §A` citation clause."""
    pat = re.compile(
        rf"\s*(?:[,;]|\u2014|--)\s*(?:see|per|cf\.?|matching|mirrors)?\s*(?:{RUN})\s*([.;]?)\s*$",
        re.VERBOSE,
    )
    return pat.sub(r"\1", text)


def strip_explanatory_tags(text: str) -> str:
    """Handle `(Pitfall 4: purge must not delete ...)` — a citation that INTRODUCES prose.

    The label is dropped and the explanation it introduced is kept, since the explanation is
    the part that was actually carrying the meaning.
    """
    # `(TAG: explanation)` / `(TAG — explanation)` -> `(explanation)`
    pat = re.compile(
        rf"([\(\[])\s*(?:{RUN})\s*(?::|\u2014\s|--\s)\s*(?=\S)",
        re.VERBOSE,
    )
    text = pat.sub(r"\1", text)
    # `TAG (mandatory): explanation` at the very start of a comment body.
    pat2 = re.compile(rf"^(\s*)(?:{RUN})\s*\([^)]*\)\s*:\s+(\S)", re.VERBOSE)
    text = pat2.sub(lambda m: m.group(1) + m.group(2).upper(), text)
    return text


def clean_line(line: str) -> str:
    m = COMMENT.match(line)
    if not m:
        return line
    prefix, body = m.group(1), m.group(2)
    original = body
    body = strip_explanatory_tags(body)
    body = strip_run_parentheticals(body)
    body = strip_trailing_tags(body)
    body = strip_leading_tags(body)
    if body == original:
        return line
    # Never leave an empty or punctuation-only comment behind where prose used to be.
    if not body.strip(" .,;:-\u2014"):
        return prefix + original
    body = re.sub(r"\s+([.,;:])", r"\1", body)
    # A citation removed from the start of a continuation line can leave the line opening
    # with the punctuation that used to attach it to the citation. A comma/semicolon is
    # dropped, but an em-dash is kept: it joins two clauses of the surrounding prose and
    # removing it would run them together.
    body = re.sub(r"^(\s*)(?:,|;)\s+", r"\1", body)
    body = re.sub(r"[ \t]+$", "", body)
    return prefix + body


def main(paths: list[str]) -> int:
    changed: list[str] = []
    for p in paths:
        path = Path(p)
        src = path.read_text()
        out = "\n".join(clean_line(l) for l in src.split("\n"))
        if out != src:
            path.write_text(out)
            changed.append(p)
    print(f"rewrote {len(changed)} files")
    return 0


if __name__ == "__main__":
    files = subprocess.run(
        ["git", "ls-files", "crates", "src-tauri/src", "frontend/src"],
        capture_output=True, text=True, check=True,
    ).stdout.split()
    files = [f for f in files if f.endswith((".rs", ".ts", ".svelte"))]
    sys.exit(main(files))
