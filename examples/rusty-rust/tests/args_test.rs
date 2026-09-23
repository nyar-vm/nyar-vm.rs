fn check(val: i32) -> bool {
    if val > 0 && val < 100 {
        return true;
    }
    return false;
}

fn main() {
    let x = 50;
    if check(x) {
        print("x is in range");
    }

    let y = 150;
    if !check(y) {
        print("y is out of range");
    }
}
