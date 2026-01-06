use duende_ublk::{UblkControl, cleanup_orphaned_devices};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing duende-ublk cleanup...");

    // Try to open control device
    let mut ctrl = UblkControl::open()?;
    println!("Opened /dev/ublk-control");

    // Try to force delete device 0
    println!("Attempting to force delete device 0...");
    match ctrl.force_delete(0) {
        Ok(true) => println!("Device 0 deleted successfully!"),
        Ok(false) => println!("Device 0 didn't exist"),
        Err(e) => println!("Error deleting device 0: {}", e),
    }

    // Also try the cleanup function
    println!("\nRunning cleanup_orphaned_devices...");
    let cleaned = cleanup_orphaned_devices()?;
    println!("Cleaned {} orphaned devices", cleaned);

    Ok(())
}
