from __future__ import annotations

import string
from pathlib import Path
from typing import List

from .cheats_types import CheatEntry, CheatFile, CheatParseError


HEX_DIGITS = set(string.hexdigits)


def _is_hex8(token: str) -> bool:
    return len(token) == 8 and all(c in HEX_DIGITS for c in token)


def parse_text(text: str, *, max_ops_per_entry: int = 1000) -> CheatFile:
    """Parse Yuzu/Citron-style cheats text into a CheatFile.

    Supported entry headers:
      - {Default} or any single-brace name appears only once (mapped to first entry)
      - [Name] standard entries; multiple allowed

    Body contains 8-hex tokens; tokens may be separated by whitespace and newlines.
    Comments are not supported by the spec.
    """
    if text is None:
        raise CheatParseError("input text is None")

    # Normalize newlines and strip BOM/spaces
    data = text.replace('\r\n', '\n').replace('\r', '\n').strip()
    if not data:
        return CheatFile(entries=[])

    entries: List[CheatEntry] = []
    current_title: str | None = None
    current_ops: List[str] = []
    current_raw_chunks: List[str] = []  # accumulate raw body including comments/spacing
    seen_default = False

    i = 0
    n = len(data)
    while i < n:
        ch = data[i]
        if ch.isspace():
            # preserve whitespace inside an entry's body
            if current_title is not None:
                current_raw_chunks.append(ch)
            i += 1
            continue

        # No comment syntax is supported; '#' or ';' should be treated as errors by default branch

        if ch == '{':
            # Commit previous entry
            if current_title is not None:
                if not current_title:
                    raise CheatParseError("empty title is not allowed")
                entries.append(CheatEntry(current_title, current_ops, _normalize_raw_body(current_raw_chunks)))
                current_title, current_ops, current_raw_chunks = None, [], []

            end = data.find('}', i + 1)
            if end == -1:
                raise CheatParseError("missing closing '}' for title")
            name = data[i + 1 : end].strip()
            if not name:
                raise CheatParseError("empty title inside '{}' braces")
            if seen_default:
                raise CheatParseError("duplicate '{...}' default-like entry")
            seen_default = True
            current_title = name
            i = end + 1
            continue

        if ch == '[':
            # Commit previous entry
            if current_title is not None:
                if not current_title:
                    raise CheatParseError("empty title is not allowed")
                entries.append(CheatEntry(current_title, current_ops, _normalize_raw_body(current_raw_chunks)))
                current_title, current_ops, current_raw_chunks = None, [], []

            end = data.find(']', i + 1)
            if end == -1:
                raise CheatParseError("missing closing ']' for title")
            name = data[i + 1 : end].strip()
            if not name:
                raise CheatParseError("empty title inside '[]' brackets")
            current_title = name
            i = end + 1
            continue

        # Hex token stream
        if ch in HEX_DIGITS:
            # Read continuous span of hex-ish token candidates
            j = i
            while j < n and data[j] in HEX_DIGITS:
                j += 1
            token = data[i:j]
            # Some files might concatenate multiple 8-hex groups; split every 8
            if len(token) % 8 != 0:
                raise CheatParseError("hex token length not multiple of 8")
            for k in range(0, len(token), 8):
                t = token[k : k + 8]
                if not _is_hex8(t):
                    raise CheatParseError("invalid hex8 token")
                if current_title is None:
                    # Implicit default section if none opened yet
                    current_title = "Default"
                if len(current_ops) >= max_ops_per_entry:
                    raise CheatParseError("too many opcodes in entry")
                current_ops.append(t)
            # also append raw token text to preserve formatting
            if current_title is not None:
                current_raw_chunks.append(token)
            i = j
            continue

        # Unknown character
        raise CheatParseError(f"unexpected character '{ch}' in cheats text")

    # Commit tail entry
    if current_title is not None:
        if not current_title:
            raise CheatParseError("empty title is not allowed")
        entries.append(CheatEntry(current_title, current_ops, _normalize_raw_body(current_raw_chunks)))

    return CheatFile(entries=entries)


def parse_file(path: Path, *, max_ops_per_entry: int = 1000) -> CheatFile:
    data = path.read_bytes()
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        # Best-effort fallback: latin-1 to preserve bytes
        text = data.decode("latin-1")
    return parse_text(text, max_ops_per_entry=max_ops_per_entry)


def serialize(model: CheatFile) -> str:
    """Serialize CheatFile back to text. Ops grouped 3 per line."""
    lines: List[str] = []
    for entry in model.entries:
        lines.append(f"[{entry.title}]")
        if entry.raw_body:
            body = entry.raw_body
            if not body.endswith('\n'):
                body += '\n'
            lines.append(body.rstrip('\n'))
        else:
            ops = entry.ops
            for i in range(0, len(ops), 3):
                lines.append(" ".join(ops[i : i + 3]))
        lines.append("")  # blank line between entries
    # Ensure final newline
    return "\n".join(lines).rstrip("\n") + "\n"


def _normalize_raw_body(chunks: List[str]) -> str | None:
    if not chunks:
        return None
    body = "".join(chunks)
    # Remove leading and trailing blank lines only; keep inner formatting/comments
    # Normalize to \n already ensured earlier
    while body.startswith("\n"):
        body = body[1:]
    while body.endswith("\n"):
        body = body[:-1]
    return body


