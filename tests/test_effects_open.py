"""Effect tags must not treat English open / comment global as side effects."""

from __future__ import annotations

from dued.effects import tag_effects


def test_english_open_in_format_not_filesystem() -> None:
    body = 'fn dispatch_company_command(technology: &str) { format!("flipped {technology} open"); }'
    tags = tag_effects(body)
    assert "filesystem" not in tags, tags


def test_file_open_and_open_options_are_filesystem() -> None:
    assert "filesystem" in tag_effects('fn load() { let _ = std::fs::File::open("x"); }')
    assert "filesystem" in tag_effects(
        'fn load() { let _ = std::fs::OpenOptions::new().read(true).open("x"); }'
    )


def test_comment_global_not_global_mutate() -> None:
    body = "fn update_customer_growth() {\n    // Calculate global customer counts\n    let n = 1;\n}"
    tags = tag_effects(body)
    assert "global_mutate" not in tags, tags
