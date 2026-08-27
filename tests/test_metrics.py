from dued.metrics import complexity
from dued.parse import parse_source


def test_python_complexity_nested_higher_than_flat() -> None:
    nested = b"""
def sum_of_primes(max_n):
    total = 0
    for i in range(1, max_n + 1):
        for j in range(2, i):
            if i % j == 0:
                break
        else:
            total += i
    return total
"""
    flat = b"""
def get_words(number):
    if number == 1:
        return "one"
    if number == 2:
        return "two"
    if number == 3:
        return "three"
    return "lots"
"""
    n = parse_source("python", ".py", nested).symbols[0]
    f = parse_source("python", ".py", flat).symbols[0]
    assert n.cognitive > f.cognitive
    assert n.cyclomatic >= 1
    assert f.cyclomatic >= 1


def test_extract_python_calls_and_docs() -> None:
    src = b'''
def helper():
    """Say hi."""
    return 1

def main():
    helper()
'''
    extracted = parse_source("python", ".py", src)
    names = {s.name for s in extracted.symbols}
    assert names == {"helper", "main"}
    helper = next(s for s in extracted.symbols if s.name == "helper")
    assert "Say hi" in helper.docstring
    assert ("main", "helper") in extracted.calls
