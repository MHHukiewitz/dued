"""Callers that raise fan_in on load_state."""

from core.engine import load_state, run_pipeline


def boot() -> str:
    return load_state("/tmp/state.json")


def refresh() -> str:
    return load_state("/tmp/state.json")


def batch(items: list[int]) -> int:
    _ = load_state("/tmp/state.json")
    return run_pipeline(items)
