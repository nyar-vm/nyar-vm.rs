
micro main() {
    until count > 0 {
        count -= 1
    }
    try {
        raise "boom"
    }
}

