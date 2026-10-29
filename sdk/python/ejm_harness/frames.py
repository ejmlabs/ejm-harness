"""JSONL frames within the frame bound."""

import json
from typing import Any, cast

from .errors import HarnessError
from .limits import MAX_FRAME


def encode_frame(value: object) -> bytes:
    frame = (json.dumps(value, ensure_ascii=False, separators=(",", ":")) + "\n").encode()
    if len(frame) > MAX_FRAME:
        raise HarnessError("frame_limit", "protocol frame exceeds 1 MiB")
    return frame


def as_object(value: object) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise HarnessError("protocol", "worker frame must be an object")
    return cast("dict[str, Any]", value)


def decode_frame(line: bytes) -> dict[str, Any]:
    """Parse one line read from worker stdout, which must be a complete JSON object."""
    if not line:
        raise HarnessError("disconnected", "worker output closed")
    if len(line) > MAX_FRAME or not line.endswith(b"\n"):
        raise HarnessError("frame_limit", "worker frame exceeds 1 MiB or is truncated")
    return as_object(json.loads(line))
