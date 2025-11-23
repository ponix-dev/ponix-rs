// cargo run --example explore --features device
use ponix_proto_prost::organization::v1;

fn main() {
    println!("Available types in organization::v1:");
    // This will show compile errors that list all available types
    let _ = v1::;
}
