"""Effect tags must not treat Rust .any(| as unsafe."""

from __future__ import annotations

from dued.effects import tag_effects


def test_iterator_any_not_unsafe() -> None:
    body = "fn resolve_deploy_anchor_pop(xs: &[u8]) -> bool { xs.iter().any(|x| *x > 0) }"
    tags = tag_effects(body)
    assert "unsafe" not in tags, tags


def test_unsafe_block_still_tagged() -> None:
    body = "fn poke(p: *const u8) -> u8 { unsafe { *p } }"
    tags = tag_effects(body)
    assert "unsafe" in tags, tags
