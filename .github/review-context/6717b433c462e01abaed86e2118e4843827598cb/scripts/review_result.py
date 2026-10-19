#!/usr/bin/env python3
"""Validate bounded AI output before it is used in a required status."""
import json
import sys
from pathlib import Path


def validate(file, head, number):
    if file.is_symlink() or not file.is_file() or file.stat().st_size > 64_000:
        raise ValueError("Missing, unsafe, or oversized review output")
    result = json.loads(file.read_text())
    if set(result) != {"head", "pass", "complete", "summary", "findings"}:
        raise ValueError("Invalid review fields")
    if result["head"] != head or type(result["pass"]) is not int or result["pass"] != number:
        raise ValueError("Review is for another head or pass")
    if not isinstance(result["summary"], str) or not 1 <= len(result["summary"]) <= 4000:
        raise ValueError("Missing or oversized review summary")
    if type(result["complete"]) is not bool:
        raise ValueError("Review completeness must be a boolean")
    if not result["complete"]:
        raise ValueError("Review is incomplete")
    findings = result["findings"]
    if not isinstance(findings, list) or len(findings) > 50:
        raise ValueError("Invalid findings")
    for item in findings:
        if not isinstance(item, dict) or set(item) != {"severity", "file", "line", "title", "reason"}:
            raise ValueError("Invalid finding fields")
        if item["severity"] not in {"critical", "high", "medium", "low"}:
            raise ValueError("Invalid severity")
        path = Path(item["file"])
        if path.is_absolute() or ".." in path.parts or not item["file"]:
            raise ValueError("Invalid finding path")
        if type(item["line"]) is not int or item["line"] < 1:
            raise ValueError("Invalid finding line")
        if any(not isinstance(item[key], str) or not 1 <= len(item[key]) <= 4000 for key in ("file", "title", "reason")):
            raise ValueError("Invalid finding text")
    return result, any(item["severity"] != "low" for item in findings)


if __name__ == "__main__":
    try:
        result, blocked = validate(Path(sys.argv[1]), sys.argv[2], int(sys.argv[3]))
        print(f"Review pass {result['pass']}: {len(result['findings'])} finding(s); " + ("blocked" if blocked else "passed"))
        # Full bounded JSON remains an artifact. Never execute or interpolate model text.
        sys.exit(1 if blocked else 0)
    except (ValueError, KeyError, OSError, TypeError) as error:
        print(f"Invalid review output: {error}", file=sys.stderr)
        sys.exit(1)
