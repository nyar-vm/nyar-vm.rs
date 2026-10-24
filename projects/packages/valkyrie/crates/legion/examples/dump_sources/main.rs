fn main() {
    use oak_valkyrie::printer::parse_source;
    let path = std::env::args().nth(1).expect("path");
    let src = std::fs::read_to_string(&path).expect("read");
    match parse_source(&src) {
        Ok(root) => eprintln!("PARSE_OK items={}", root.items.len()),
        Err(e) => {
            eprintln!("PARSE_ERR");
            eprintln!("{e}");
        }
    }
}
