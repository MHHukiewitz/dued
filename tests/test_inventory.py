from pathlib import Path

from dued.cost import cost_hint
from dued.inventory import package_map
from dued.risks import tag_risks


def test_package_map_finds_pyproject() -> None:
    packs = package_map(Path(__file__).resolve().parents[1])
    kinds = {p["kind"] for p in packs}
    assert "python" in kinds


def test_risk_and_cost() -> None:
    tags = tag_risks("check_password", "def check_password():\n    hmac.new(b'x')\n", "def check_password()")
    assert "auth" in tags
    assert "crypto" in tags
    assert cost_hint("for x in items:\n    open(path)\n") >= 3
