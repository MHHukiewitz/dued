from dued.graph import choose_call_targets


def test_new_same_file_only() -> None:
    langs = {1: "rust", 2: "rust"}
    targets = [(10, 1), (11, 2)]
    assert choose_call_targets("new", targets, 1, langs) == [(10, 1)]
    assert choose_call_targets("new", targets, 9, langs) == []


def test_unique_name_resolves_across_files() -> None:
    langs = {1: "rust", 2: "rust"}
    assert choose_call_targets("get_user", [(20, 2)], 1, langs) == [(20, 2)]


def test_ambiguous_name_does_not_fan_out() -> None:
    langs = {1: "rust", 2: "rust"}
    assert choose_call_targets("process", [(20, 2), (21, 2)], 1, langs) == []


def test_multiple_same_file_new_does_not_fan_out() -> None:
    langs = {1: "rust", 2: "rust"}
    assert choose_call_targets("new", [(10, 1), (12, 1), (11, 2)], 1, langs) == []
