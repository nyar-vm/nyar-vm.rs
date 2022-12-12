func main() {
    defer func() {
        if r := recover(); r != nil {
            fmt.Println("recovered from:", r)
        }
    }()
    
    fmt.Println("starting")
    panic("something went wrong")
    fmt.Println("this will not be printed")
}
