use dued_rs::parse::parse_source;

#[test]
fn python_nested_has_higher_cognitive() {
    let nested = br#"
def sum_of_primes(max_n):
    total = 0
    for i in range(1, max_n + 1):
        for j in range(2, i):
            if i % j == 0:
                break
        else:
            total += i
    return total
"#;
    let flat = br#"
def get_words(number):
    if number == 1:
        return "one"
    if number == 2:
        return "two"
    if number == 3:
        return "three"
    return "lots"
"#;
    let n = parse_source("python", ".py", nested);
    let f = parse_source("python", ".py", flat);
    assert!(n.symbols[0].cognitive > f.symbols[0].cognitive);
    assert!(n.symbols[0].cyclomatic >= 1);
}

#[test]
fn extracts_python_calls() {
    let src = br#"
def helper():
    """Say hi."""
    return 1

def main():
    helper()
"#;
    let extracted = parse_source("python", ".py", src);
    let names: Vec<&str> = extracted.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"helper"));
    assert!(names.contains(&"main"));
    assert!(extracted.calls.iter().any(|(o, c)| o == "main" && c == "helper"));
}

#[test]
fn rust_impl_method_calls_imported_fn() {
    let src = br#"
use mainnet_graph::allocate_customers;

pub struct GameState {
    world: i32,
}

impl GameState {
    fn apply_competitive_alloc_jobs(&mut self, demand: i32) {
        let _ = allocate_customers(&mut self.world, demand);
    }

    pub fn sync_graph_competitive_allocate(&mut self, demand: i32) {
        self.apply_competitive_alloc_jobs(demand);
    }
}
"#;
    let extracted = parse_source("rust", ".rs", src);
    let names: Vec<&str> = extracted.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"apply_competitive_alloc_jobs"));
    assert!(names.contains(&"sync_graph_competitive_allocate"));
    assert!(
        extracted
            .calls
            .iter()
            .any(|(o, c)| o == "apply_competitive_alloc_jobs" && c == "allocate_customers"),
        "impl method must record call to imported crate fn: {:?}",
        extracted.calls
    );
}
