pub fn allocate_customers(world: &mut i32, demand: i32) -> i32 {
    *world += demand;
    demand
}

pub fn prototype_month() {
    let mut world = 0;
    let _ = allocate_customers(&mut world, 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates() {
        let mut world = 0;
        let _ = allocate_customers(&mut world, 2);
    }
}
