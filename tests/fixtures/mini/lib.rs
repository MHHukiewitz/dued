use std::fs;

pub fn get_user(id: u64) -> String {
    fs::read_to_string(format!("/tmp/{id}")).unwrap_or_default()
}

pub fn process(items: &[i32]) -> i32 {
    let mut total = 0;
    for item in items {
        if *item > 0 {
            total += item;
        }
    }
    total
}

fn unused_rs() {}

fn main() {
    let _ = get_user(1);
    let _ = process(&[1, 2, 3]);
}
