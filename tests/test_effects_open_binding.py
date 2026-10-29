"""A local binding named `open` must not be tagged filesystem."""

from __future__ import annotations

from dued.effects import tag_effects


def test_open_local_binding_not_filesystem() -> None:
    bodies = [
        "fn unreachable_cells(&self) -> Vec<Cell> { let open = |x: i32, y: i32| -> bool"
        " { self.is_inside(x, y) }; if open(n.0, n.1) { out.push(n); } out }",
        "fn walk(open: impl Fn(i32) -> bool) -> bool { open(x) }",
        "fn walk(xs: &[i32]) { xs.iter().for_each(|open| { open(x); }) }",
        "def walk(cells, open):\n    return [c for c in cells if open(c)]\n",
        "def walk(cells):\n    open = cells.get\n    return open(cells)\n",
        "function walk(cells) { const open = (c) => c > 0; return open(cells); }",
        "const walk = (cells, open) => open(cells);",
    ]
    for body in bodies:
        tags = tag_effects(body)
        assert "filesystem" not in tags, (body, tags)


def test_real_open_calls_are_filesystem() -> None:
    bodies = [
        "def load(path):\n    with open(path) as f:\n        return f.read()\n",
        "def load(path):\n    f = open(path)\n    return f\n",
        "fn load(p: &Path) { let _ = File::open(p); }",
        "fn load(p: &Path) { let _ = OpenOptions::new().read(true).open(p); }",
        "fn load(p: &Path) { let _ = fs::open(p); }",
        "def load(path):\n    return os.open(path, 0)\n",
    ]
    for body in bodies:
        tags = tag_effects(body)
        assert "filesystem" in tags, (body, tags)
