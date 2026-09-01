use tinyvec::TinyVec;
fn main() {
    println!("TinyVec<[u8;24]> = {}", size_of::<TinyVec<[u8; 24]>>());
    println!("TinyVec<[u8;32]> = {}", size_of::<TinyVec<[u8; 32]>>());
    println!("TinyVec<[u8;40]> = {}", size_of::<TinyVec<[u8; 40]>>());
    println!("TinyVec<[u8;44]> = {}", size_of::<TinyVec<[u8; 44]>>());
    println!("TinyVec<[u8;46]> = {}", size_of::<TinyVec<[u8; 46]>>());
    println!("TinyVec<[u8;48]> = {}", size_of::<TinyVec<[u8; 48]>>());
    println!("TinyVec<[u8;64]> = {}", size_of::<TinyVec<[u8; 64]>>());
}
