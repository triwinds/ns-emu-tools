from __future__ import annotations

from typing import List, NamedTuple, Optional


class CheatEntry(NamedTuple):
    """Represents a single cheat entry consisting of a title and a list of opcodes.

    The opcode list contains strings of exactly 8 hexadecimal characters (uppercase or lowercase).
    """

    title: str
    ops: List[str]
    # Original raw body text between header and next header/end, if available.
    raw_body: Optional[str] = None


class CheatFile(NamedTuple):
    """A parsed cheats file consisting of an ordered list of entries."""

    entries: List[CheatEntry]


class CheatParseError(Exception):
    """Raised when a cheats file cannot be parsed according to the Yuzu/Citron format."""

    pass


