use duende_mlock::{is_locked, lock_all, locked_bytes};

fn main() {
    println!(
        "Before: is_locked={}, bytes={}",
        is_locked(),
        locked_bytes()
    );
    match lock_all() {
        Ok(status) => println!("lock_all returned Ok: {:?}", status),
        Err(e) => println!("lock_all returned Err: {:?}", e),
    }
    println!("After: is_locked={}, bytes={}", is_locked(), locked_bytes());
}
