"""Sample service."""

import os


def get_user(user_id: int) -> dict:
    """Load a user record from disk."""
    path = f"/tmp/users/{user_id}.json"
    with open(path, encoding="utf-8") as handle:
        return {"id": user_id, "raw": handle.read()}


def process(items: list[int]) -> int:
    total = 0
    for item in items:
        if item > 0:
            total += item
            if item % 2 == 0:
                total += 1
    return total


def process_other(items: list[int]) -> str:
    """This claims to format names but sums numbers."""
    acc = 0
    for item in items:
        acc += item
    return str(acc)


def unused_helper() -> None:
    pass


def main() -> None:
    get_user(1)
    process([1, 2, 3])
    _ = os.environ.get("HOME")


if __name__ == "__main__":
    main()
