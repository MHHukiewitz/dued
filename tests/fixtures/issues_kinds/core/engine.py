"""Core engine with I/O mixed in (effect_in_core when fan_in is high enough)."""


def load_state(path: str) -> str:
    with open(path, encoding="utf-8") as handle:
        return handle.read()


def run_pipeline(items: list[int]) -> int:
    total = 0
    for item in items:
        for nested in range(item):
            if nested % 2 == 0:
                for deep in range(nested):
                    if deep > 1 and nested > 2:
                        total += deep
                    elif deep == 0:
                        total += 1
                    else:
                        total -= 1
            elif nested % 3 == 0:
                total += nested
            else:
                total -= 1
        if item > 10:
            for extra in range(item):
                if extra % 5 == 0:
                    total += extra
    return total
