use std::env;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let total = harbor::order_total(&[1200, 450], &[2, 1]);
    println!("order total: {} cents", total);
    if args.iter().any(|a| a == "--help") {
        println!("usage: harbor [--help]");
    }
}
