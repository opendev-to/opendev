use std::sync::Arc;
use std::thread;

fn main() {
    let mut vec = vec![1, 2, 3];
    let v2 = vec.clone();
    println!("{:?}", vec);
}
