fn main() {
    println!("whoami::username(): {}", whoami::username());
    match whoami::hostname() {
        Ok(host) => println!("whoami::hostname(): {}", host),
        Err(e) => println!("whoami::hostname() error: {}", e),
    }
}
