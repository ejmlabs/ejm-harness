"""Errors raised by the SDK."""


class HarnessError(Exception):
    """A worker error or a bounded transport failure."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code
