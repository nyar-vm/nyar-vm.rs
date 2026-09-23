package main

func main() {
    var x = 10
    var y = 20
    var z = x + y
    println("x + y =", z)

    if z > 20 {
        println("z is greater than 20")
    } else {
        println("z is not greater than 20")
    }

    for i := 0; i < 5; i = i + 1 {
        println("i =", i)
    }
}
