"""A local variable named `fs` must not be tagged filesystem."""

from __future__ import annotations

from dued.effects import tag_effects


def test_fs_local_variable_not_filesystem() -> None:
    rust = "fn tick(fs: &mut Fleet) { fs.remaining_days = fs.remaining_days.saturating_sub(1); }"
    py = "def tick(fs):\n    fs.remaining_days = fs.free_sub(1)\n"
    js = "function tick(fs) { fs.remaining_days = fs.len(); }"
    for body in [rust, py, js]:
        tags = tag_effects(body)
        assert "filesystem" not in tags, tags


def test_fs_paths_and_node_apis_are_filesystem() -> None:
    assert "filesystem" in tag_effects('fn load() { let _ = std::fs::read("x"); }')
    assert "filesystem" in tag_effects('fn save() { let _ = fs::write("x", b"y"); }')
    assert "filesystem" in tag_effects(
        'use std::fs;\nfn load() { let _ = fs::read_to_string("x"); }'
    )
    assert "filesystem" in tag_effects('async fn load() { let _ = tokio::fs::read("x").await; }')
    assert "filesystem" in tag_effects('function load() { fs.readFileSync("x"); }')
