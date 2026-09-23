enum Message {
    Quit,
    Move { x: i32, y: i32 },
    Write(String),
}

fn process_message(msg: Message) {
    match msg {
        Message::Quit => print("quit"),
        Message::Move { x, y } => print("move"),
        Message::Write(s) => print(s),
    }
}
