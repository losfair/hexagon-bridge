use hexagon_vm_core::value::Value;

#[test]
fn print_layout() {
    println!("--- ORT data layout ---");
    println!("Size of value::Value: {}", ::std::mem::size_of::<Value>());
    println!("--- END ---");
}
