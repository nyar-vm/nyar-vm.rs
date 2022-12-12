fn test_defer() {
    print("start");
    defer print("end");
    print("middle");
}

fn test_errdefer(fail: bool) !void {
    print("open resource");
    errdefer print("close resource on error");
    
    if (fail) {
        return error.SomeError;
    }
    print("success");
}
