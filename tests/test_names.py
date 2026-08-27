from dued.fingerprints import fingerprint_overlap, fingerprint_symbol
from dued.hollow import is_hollow
from dued.names import tokenize_name


def test_tokenize_camel_and_snake() -> None:
    assert tokenize_name("get_user") == ["user"]
    assert tokenize_name("UserModel") == ["user", "model"]


def test_fingerprint_overlap_same_is_one() -> None:
    fp = fingerprint_symbol("process", ["filesystem"], 1, 2, 3, 4, ["open"])
    assert fingerprint_overlap(fp, fp) == 1.0


def test_hollow_pass_body() -> None:
    assert is_hollow("def unused_helper() -> None:\n    pass\n", "") == "empty_body"
